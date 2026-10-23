# magit-lsp — Roadmap

Canonical agent-facing reference for the roadmap, design, protocol, decisions,
implementation status and backlog. Written so that a less capable model can
implement each item without guessing: every term is defined in the glossary
(§1), every protocol shape is spelled out in §3, and every task states what
"done" means. Read selectively: use [AGENTS.md](./AGENTS.md) as the concise
entry point and consult only the relevant sections here. `README.md` owns
user-facing documentation; do not duplicate detailed design/history in
`AGENTS.md`.

**Status note:** design sections describe intended behaviour; §10 records
implementation status and deviations at its stated point in time. Verify
current behaviour in source/tests rather than treating historical notes as
current facts.

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
10. Implementation status and deviations
11. Client reviews
12. Unchecked backlog by category and priority

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
| Minimum Git version | Git 2.19 is a candidate minimum pending CI validation, not a settled supported minimum |

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

- [x] Done.

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
5. **Confirm** (`common.confirm` in the editing scope): the client saves the
   file (if `realFile`) and sends `view/invoke common.confirm`; the server
   replies `ok` to the helper; Git continues. **Cancel** (`common.cancel`):
   server replies `abort`. **Closing the buffer/tab without confirming = cancel.**
   Cancel currently aborts immediately; a discard-confirm prompt for a non-empty
   message is not implemented.
6. The implemented bridge edits real Git-owned files (`mode: "realFile"`). It
   currently emits only commit messages. Merge/tag messages and `rebase-todo`
   are not emitted yet; in particular, virtual documents and a structured
   rebase-todo editor are not implemented. Do not send `mode: "virtual"` until
   that editor and its write-back path exist.
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

Visibility has four levels: section (staged/unstaged) → files → hunks →
individually visible hunks. `standard` binds `alt+1` … `alt+4` to set level N;
`vim` uses `z m`/`z r` to step the level down/up. §5.1, profile bindings and
tests must agree with this hierarchy.

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
| **Log** | `magit-log` | Commit list with graph, decorations, author, date; options (`-n`, `--graph`, `--decorate`, `--author`, `--since`, `--grep`, `-S`, `-G`, path limits); lazy "load more"; **select mode** | show commit, checkout/reset/branch/rebase/cherry-pick/revert from the commit at point, confirm/cancel | 5 |
| **Log variants** | `l` menu | current, other, all, local branches, reflog, file log, line-range log (`-L`), `HEAD..upstream`, `upstream..HEAD` | same | 5 |
| **Revision** | `magit-revision` | Header (hash, parents, author, committer, dates, refs, message), diffstat, full diff with hunks; parent/child navigation; combined diff for merges | visit file, apply/reverse hunk, copy hash | 5 |
| **Diff** | `magit-diff` | Any range `a..b`, `a...b`, worktree vs index vs revision, path-limited, stash vs parent, stat-only; context/whitespace/rename/word-diff options | visit, stage/unstage when applicable | 5 |
| **Refs** | `magit-show-refs` | Local branches, remotes, tags with ahead/behind vs a base; also select mode | checkout, create, delete, rename, reset, merge, rebase onto, log | 5 |
| **Cherry** | `magit-cherry` | `git cherry` between two refs | cherry-pick, show | 7 |
| **Stash list / show** | `magit-stash-list` | Stashes; show = diff view of a stash | apply, pop, drop, branch | 6 |
| **Rebase todo** | `git-rebase-mode` | Planned structured interactive todo: pick/reword/edit/squash/fixup/drop/exec/label/merge lines with commit summaries; reorder | move up/down, set action, show commit, confirm, abort | 7 |
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

### Global unchecked backlog — priority order

The task specifications and checkboxes remain in §§7 and 11; this index makes
the categories and ordering global. Complete all Review items before Feature
work. Bugfix work is a distinct category, not a scenario-behavior task.

#### Review (highest priority; grouped by stated priority)

- **P1:** Test editor-bridge cancellation; strengthen IMM-2 property tests;
  extract shared stdio client; build `TestSession`; verify askpass secrets are
  not logged; add missing real-UI scenarios; test resize reflow.
- **P2:** Fix keymap rule checks; resolve detail-level mismatch; authoritative
  parity keys; test unavailable/scope mismatch; acked versions; keymap delivery;
  cross-scope conflicts; config gaps; minimum Git version; editor disconnect;
  Neovim prompts; option clearing/saved args; VSCode keybinding drift; remove
  Lua sleeps; strengthen read-only assertions; mutation ordering; prune duplicate
  tests; reduce snapshots; remove magic counts; tighten assertions; remove
  global environment mutation; remaining coverage gaps.
- **P3:** UI latency assertions; rendered-text parsing in tests; housekeeping.
- Client review follow-ups: R-NV-1…R-NV-4 and R-VS-1.
- R-UI-7 status placement (from the Neovim client review).

#### Bugfix

- **P1:** Diagnose Neovim harness startup/socket creation and
  connection independently of scenario behavior (§11.3).

#### Feature

- R-3.4b editor bridge clients/askpass; R-3.5…R-3.7;
  R-4.1…R-4.3; R-5.1…R-5.5; R-6.1…R-6.5; R-7.1…R-7.4; R-8.1…R-8.3;
  R-9.1…R-9.5. Their full acceptance criteria remain in the phase entries.

### Task ticket template and UI rule

Every implementation task is dispatched as a TASK TICKET with these fields:

- **Target Files** — the only files the worker may create or modify.
- **Reference Files** — what to read first.
- **Architectural Constraints** — invariants the change must not break.
- **Acceptance Criteria** — observable, checkable outcomes.
- **Verification Command** — the exact command that must exit 0.

For any task that changes what the user sees (`ui-test-plan.md` U2.1):

- **Acceptance Criteria** must contain a visible outcome: "after pressing X
  the screen shows …".
- **Verification Command** must include `make ui-test`.
- The report lists the scenario files, their results and the saved `.screen`
  paths.
- A task is not done until a `.scn` scenario has driven the real UI
  (`scripts/ui`, §8.8).

Every task that opens a new view (log, revision, refs, commit message) also
adds a scenario for it.

### Phase 0 — Groundwork

- [x] R-0.1 IMM-1…IMM-9 complete.
- [x] R-0.2 `git status --porcelain=v2 -z --branch` parsing; paths with spaces/
      quotes/unicode; rename old/new paths. *Done when:* fixtures with each of
      those pass.
- [x] R-0.3 Separate **Untracked files** section (currently lumped in Unstaged
      with status `?`); stage/unstage them via UI; `stage_all` also stages
      untracked when `status.stage-all` is invoked on the Untracked header.
      *Done when:* snapshot of a repo with untracked files shows the section.
- [x] R-0.4 `patch.rs`: original headers preserved; renames, mode changes,
      symlinks, binary (whole-file only), no-trailing-newline, CRLF. *Done when:*
      one fixture and test per case.
- [x] R-0.5 Hunk identity by content hash everywhere (no hunk indexes in any
      API). *Done when:* a test mutates the file between render and action and
      the action targets the right hunk or is rejected with a message.
- [x] R-0.6 `tracing` to stderr/file; child stdout never reaches the LSP pipe.
- [x] R-0.7 Move the stdio loop to `tokio`.

### Phase 1 — Server-owned document and sessions

- [x] R-1.1 Node tree, Node IDs, renderer for Status in Rust producing text,
      tokens, symbols, links, line→node map. *Done when:* a snapshot test
      shows the full status text; deleting `src/document.rs` parsing breaks
      nothing.
- [x] R-1.2 `Session`, `View` trait, `ViewStack`, `view/open`, `view/render`
      (full + line edits), `view/rendered` ack, `view/resync`, cursor
      successor by Node ID. *Done when:* a test refreshes after a mutation and
      the returned cursor is on the same node.
- [x] R-1.3 Semantic tokens from the tree (§3.15 mapping); Neovim defines
      `@lsp.type.magit.*` links.
- [x] R-1.4 `documentSymbol` replaces `vcs/targets`; `definition`/`showDocument`
      for visit.
- [x] R-1.5 Server-owned visibility with lazy bodies; `common.toggle-section`,
      level commands.
- [x] R-1.6 Delete `vcs/didRender`, `vcs/status` and all old `vcs/*` methods;
      the Lua client keeps only buffer replacement, key forwarding and
      window management.

### Phase 2 — Commands, keymaps, config, text mode, menu framework, second client

- [x] R-2.1 Command registry (`id`, `scope`, `doc`, `status`, `run`).
- [x] R-2.2 Keymap resolution, profiles `standard` and `vim` (§5), TOML config
      with `include`/`include_optional`, XDG location, `--config`,
      `MAGIT_LSP_CONFIG`, hot reload, validation messages.
- [x] R-2.3 `view/key`, `view/invoke`, `view/keymap`, `passThrough`, capture mode.
- [x] R-2.4 Neovim: atomic keymap apply (tracked buffer-local set), visual
      selection forwarding, capture mode, fold-key mirroring (§3.9, incl.
      scanning user mappings), default profile `vim`.
- [x] R-2.5 `magit-lsp keymap --format json|vscode|neovim|markdown --check`;
      CI snapshot; workbench deny-list test.
- [x] R-2.6 Text mode end to end (§3.14); `common.copy-value`.
- [x] R-2.7 **Menu framework** (§3.11): model, text rendering with columns from
      `viewport`, switches/options/actions, argument preview, structured model
      in `view/render`, `bottom` placement in Neovim (float and split).
- [x] R-2.8 **Planned-feature menus** (§3.12): create every dispatch menu
      with its full Magit action list as `Planned`; `open.dispatch` view
      generated from the registry; `cargo xtask parity`.
- [x] R-2.9 **Second client to prove decoupling:** a minimal VSCode extension
      (static keybindings + context keys + QuickPick menu + fold-key mirroring
      command) or a ratatui TUI. Fix protocol gaps found here before Phase 3. The complexity of implementing a language server will be a factor here. It is important that clients integrations can be very thin.

### Phase 3 — Effects, prompts, runner, process log, editor bridge

- [x] R-3.1 `Ui` handle and `view/prompt` with the four kinds (§3.6); Neovim
      and VSCode mappings; `ScriptedUi` for tests. Menu option prompts now work.
- [x] R-3.2 Process runner (queue, progress, cancel, ring buffer).
- [x] R-3.3 **Process log view.**
- [x] R-3.4 Editor bridge (core): helper subcommand, socket, `editDocument`
      effect, confirm/cancel semantics (§3.10). *Done when:* a Rust test runs
      `git commit` with the fake client answering the `editDocument` effect and
      the commit has the typed message. Neovim/VSCode document handling and
      askpass remain a follow-up; `--askpass` is a usage error for now.

### Phase 3U — Real-UI harness and UI bug fixes (do before R-3.5)

The step lists (U0.x, U1.x, U2.x), the bug table and the scenario DSL live in
`ui-test-plan.md`.

- [x] R-UI-1 **Real-UI harness** (U0.1–U0.9): tmux driver `scripts/ui`
      (`start`, `keys`, `wait`, `screen`, `state`, `trace`, `stop`, `run`),
      `lua/magit-lsp/debug.lua` (`snapshot()`, `idle()`, `MAGIT_LSP_TRACE`),
      ANSI decoder, deterministic fixtures, scenario runner (`scripts/ui_lib/`)
      and the Make targets `ui-test`, `ui-tour`, `ui-unit`. Fixed the
      render-before-open race (the client buffers `view/render`/`view/keymap`
      per `viewId` until the view exists).
      *Results:* 3 PASS / 7 XFAIL over the ten bug scenarios (bug 3 passes and
      is a plain regression). Bug 5 did not reproduce: the measured `?` path is
      11–16 ms end to end, so `latency-menu-open` is a plain passing regression
      guard. The `commit-failure-clears-progress` scenario arrived with R-UI-5.
      Scenarios: `tests/ui/smoke.scn`, `tests/ui/tour.scn`, every file in
      `tests/ui/scenarios/`.
      *Done when:* `make ui-test` runs both profiles at 100x30 and 80x24 and
      reports PASS / FAIL / XFAIL; the bug scenarios stay `xfail` until
      R-UI-2…R-UI-5 remove their tags; `make test`, `make ui-unit` and
      `make ui-test` green.
- [x] R-UI-2 **Help and menu stack** (bugs 1, 2, 3; U1.1–U1.3): `?` and `h`
      open the dispatch from status, `?` inside a menu does not stack a second
      menu, a top-level menu replaces the current one, and `q`/`Escape` close a
      menu and leave status focused.
      Scenarios: `help-from-status` (both profiles, `?`), `help-from-status-h`
      (standard-only, `h`), `menu-then-help`, `menu-close-keys`.
      *Done when:* remove the `xfail` tag from the listed scenario(s); `make
      test` and `make ui-test` green. *Results:* 6 PASS / 5 XFAIL / 0 FAIL
      (`make ui-test`); the two `xfail` tags are gone and the five remaining
      XFAILs belong to R-UI-3…R-UI-5.
- [x] R-UI-3 **Read-only warning** (bug 4; U1.4–U1.6): drop `readonly` (keep
      `modifiable=false`, `buftype=nofile`, `undolevels=-1`); no W10 on entry
      or on any key; text mode still restores modifiability.
      Scenarios: `no-readonly-warning` plus the text-mode entry/exit scenario
      from U1.6.
      *Done when:* remove the `xfail` tag from the listed scenario(s); `make
      test` and `make ui-test` green. *Results:* the `xfail` tag is gone;
      `no-readonly-warning` and `text-mode-roundtrip` PASS under both profiles
      at 100x30 and 80x24; `make ui-test` reports 8 PASS / 4 XFAIL / 0 FAIL
      (the remaining XFAILs belong to R-UI-4/R-UI-5). The driver's
      `FileChangedRO` workaround is removed.
- [x] R-UI-4 **Menu rendering: actions, flag colours, pending chords** (bugs 6,
      7, 8, 10; U1.7–U1.9): every menu shows all its action keys at width 80
      and 120, floats fit or scroll, `magitMenuFlagOn`/`FlagOff` colour the
      `(--flag)` text, and the server carries the `pending` chord so
      non-matching items dim and `Backspace` pops a step.
      Scenarios: `commit-menu-shows-actions`, `flag-colours`, `chord-prefix`,
      plus the generated all-menus test.
      *Done when:* remove the `xfail` tag from the listed scenario(s); `make
      test` and `make ui-test` green. *Results:* the `xfail` tag is gone;
      `chord-prefix` passes under both profiles at 100x30 and 80x24, and
      `make ui-test` reports 11 PASS / 1 XFAIL / 0 FAIL (only
      `commit-clean-tree`, owned by R-UI-5). The menu keeps `pending` as a
      generic step list; `view/render` carries `pending` and `view/keymap`
      carries `pending` + `liveKeys`, so the client holds no chord logic.
      Screens: `build/ui/chord-prefix/{standard,vim}-{100x30,80x24}/{prefix,popped,toggled}.screen`.
- [x] R-UI-5 **Commit feedback and latency** (bugs 9, 5; U1.10–U1.11):
      pre-flight `commit.create` (clean tree → `Nothing to commit (working
      tree clean)` with an `--allow-empty` hint; unstaged-only → stage-all
      confirm), always end progress in a `finally`, and keep the measured
      latency budgets with `latency-menu-open` as a regression guard.
      Scenarios: `commit-clean-tree`, `commit-failure-clears-progress` (new; a
      failing hook shows an error and no `Committing`), `latency-menu-open`.
      *Done when:* remove the `xfail` tag from the listed scenario(s); `make
      test` and `make ui-test` green. *Results:* all three scenarios PASS under
      both profiles at 100x30 and 80x24; `make ui-test` reports 13 PASS / 0
      XFAIL / 0 FAIL. The commit worker owns a `$/progress` token ended by a
      drop guard, the client registers `view/effects` so detached results reach
      the user, and the trace merges client/server events on `t_unix_ms`. `?`
      from status on `modified-two-files`: key → server 0.33 ms, handler 1.34 ms,
      server → first menu window 5.0 ms, three renders 6.6 ms, ~13.3 ms end to
      end. Screens: `build/ui/commit-clean-tree/{standard,vim}-{100x30,80x24}/clean-commit.screen`,
      `build/ui/commit-failure-clears-progress/*/hook-failure.screen`,
      `build/ui/latency-menu-open/*/{dispatch,commit-menu,toggled,staged}.screen`.
- [x] R-UI-6 **Tour and CI** (U2.2, U2.3): extend `tests/ui/tour.scn` to cover
      status, `?`, each menu, a toggled switch, a pending chord, a clean-tree
      commit and an error; it writes `build/ui/tour.md`. Add the CI job that
      runs `make ui-test` with tmux and Neovim installed and uploads
      `build/ui/` on failure.
      Scenarios: `tests/ui/tour.scn`, `tests/ui/tour-clean.scn`,
      `tests/ui/tour-error.scn`, `tests/ui/smoke.scn` and all of
      `tests/ui/scenarios/`.
      *Done when:* the tour renders every listed screen and the CI job is
      green; remove the `xfail` tag from the listed scenario(s) (the tour and
      smoke scenarios are plain, so none should be tagged); `make test` and
      `make ui-test` green.
      *Results:* `make ui-tour` runs the three tour files into
      `build/ui/tour.md`: status, the dispatch, all 21 implemented menus, a
      pending chord, a toggled switch, an unavailable-command message, a
      clean-tree commit and a failing-hook error. `make ui-test` reports
      13 PASS / 0 XFAIL / 0 FAIL. The new `ui-test` CI job installs tmux and
      Neovim, builds the server, runs `make ui-unit ui-test` with `CI=1` and
      uploads `build/ui/` on failure.
- [ ] R-UI-7 **Status fills the window by default**: `setup()` must no longer
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
- [ ] R-VS-1 **VSCode extension: build, test and document** (see §11.2): a
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
- [ ] R-3.4b **Editor bridge clients and askpass** (U2.4): handle
      `editDocument` in Neovim and VSCode, implement `--askpass`, and add the
      `commit-message-editor` scenario (`c c` with a staged change → bottom
      split with a `gitcommit` buffer → type and confirm → clean tree and the
      commit in the log). Runs after R-UI-5, which provides the
      `commit.create` pre-flight.
      Scenarios: `commit-message-editor`.
      *Done when:* remove the `xfail` tag from the listed scenario(s); `make
      test` and `make ui-test` green.

### Phase 3N — Neovim client debt (findings in §11.1; do before R-3.5)

- [ ] R-NV-1 **Async request layer** (§11.1 finding 1): replace
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
- [ ] R-NV-2 **One view manager** (§11.1 finding 2): replace the three parallel
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
- [ ] R-NV-3 **Isolate debug instrumentation** (§11.1 finding 3): product code
      calls `debug_api.*` about 35 times. Route them through one thin hook
      table (`require('magit-lsp.hooks')` with no-op defaults) that
      `debug.lua` fills only when `MAGIT_LSP_TRACE` or the test harness is
      active; `debug.lua` must not be required on a normal start.
      *Done when:* a normal `setup()` never loads `debug.lua`
      (L2 assert on `package.loaded`); `make ui-test` green.
- [ ] R-NV-4 **Structure and defaults** (§11.1 findings 4–7): split `init.lua`
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

- [ ] R-3.5 File watcher auto-refresh; `index.lock` handling.
      *UI:* add scenario `auto-refresh`: modify a tracked file outside Neovim,
      wait, and the status screen shows the new change without
      `common.refresh`.
- [ ] R-3.6 Discard (`status.discard`) for files, hunks, selections, with
      `confirm` prompt (`destructive: true`); consider saving discarded content
      under a private ref for undo.
      *UI:* add scenario `discard-confirm`: `x` on a hunk shows the destructive
      confirm prompt; `n` leaves the change, `y` removes it from the screen.
- [ ] R-3.7 `UserError` mapping table for all remaining Git failures as they
      are met.

### Phase 4 — Commit flow

- [ ] R-4.1 Commit menu: commit, extend, amend, reword, fixup, squash, augment,
      instant fixup/squash, alter; switches `--all`, `--allow-empty`,
      `--verbose`, `--no-verify`, `--signoff`, `--gpg-sign`, `--reset-author`,
      option `--author=`. Items flip from `Planned` to `Implemented`.
      *UI:* extend `commit-menu-shows-actions` with switch toggling (`-a` flips
      the `--all` flag) and the `c c` flow; after pressing `c` the screen lists
      `Commit`, `Extend`, `Amend`, `Reword`, `Fixup`, `Squash`.
- [ ] R-4.2 Commit-message view over the editor bridge: staged-diff companion
      view, history ring, trailers, lint diagnostics, confirm/cancel/close.
      *UI:* add scenario `commit-message-view`: staged change, `c c`, a bottom
      split shows the `gitcommit` buffer and the staged diff; confirm closes
      it and the status screen shows a clean tree.
- [ ] R-4.3 Status "Recent commits" section.
      *UI:* add scenario `recent-commits`: the status screen shows a "Recent
      commits" section with a commit subject; `Enter` visits the revision.

### Phase 5 — Read views

- [ ] R-5.1 Log (graph, decorations, filters, lazy load) and select mode (§3.7).
      *UI:* add scenario `log-view`: `l l` opens the log, the screen shows the
      commit subject, `q` returns to status.
- [ ] R-5.2 Revision view; Diff view (ranges, args).
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
   implemented as written. It is **not** sufficient now that highlighting
   comes from LSP semantic tokens (§3.15's target), because those are skipped;
   the `nvim.lsp.semantic_tokens.get_at_pos()` dump (plus `nvim_get_hl`) is the
   only way to recover the semantic class, since HTML classes carry only the
   resolved group name. `scripts/color_spike.lua` therefore dumps the tokens
   and writes `build/test-output/semantic-tokens.md` (group → link → resolved
   fg per scheme); the HTML files remain a visual reference only.

**Additional finding (mapping bug, now fixed):** the old client linked
`MagitDiffAdded` / `MagitDiffRemoved` to `diffAdded` / `diffRemoved`, which do
**not** exist in Neovim's built-in schemes (`hlexists` = 0; resolved fg is nil
in all six tested cases), so added/removed lines rendered uncoloured and the
HTML carried an empty `.diffAdded {}` rule. The semantic-token links use
`@diff.plus` → `Added` and `@diff.minus` → `Removed`, which every built-in
scheme defines. Note also that `Title` in `habamax` and `lunaperche` has no
`guifg` (bold only), so "no foreground" is not by itself a failure — the §8.4
assertions must inspect all resolved attributes.

Spike output: `build/test-output/status-<scheme>-<background>.html` plus
`build/test-output/semantic-tokens.md` (the token group → link → resolved fg
table) printed by `make color-spike`.

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

---

## 10. Implementation status and deviations (written after R-2.8)

**Done:** Phases 0, 1, R-2.1…R-2.9, R-3.1…R-3.4 (core), R-UI-1…R-UI-6
(`make test` green: Rust + Neovim mini.test + the VSCode node tests;
`make ui-test` green with 13 PASS / 0 XFAIL; `make ui-tour` writes
`build/ui/tour.md`). The VSCode client is the thin second client for R-2.9;
node 20 / npm exist, `tsc` and `code` do not. R-3.4 is the server core only:
the Neovim/VSCode `editDocument` handling and `--askpass` are R-3.4b.
**Not started:** R-3.4b, R-3.5…R-3.7 and Phases 4–9.

**Process notes:** the worker's first two dispatches failed with "Model
unavailable (deepseek-flash-latest)" and succeeded once the model was fixed.
Disk was ~99% full; the R-2.4 worker set `[profile.dev]`/`[profile.test]`
`debug = "line-tables-only"`, `incremental = false` and ran `cargo clean`
(`target/` 4.8 GB → ~1 GB). Keep `target/` small.

**Where the implementation differs from the spec (worker's reason in brackets):**

- **IMM-1:** the `git rm --cached -f` fallback exists but no test reaches it
  (`git reset -q --` already works on unborn HEAD). `unstage_hunk` emits a
  `/dev/null` new-file header when reversing a new file (else an empty staged
  entry stays). README records only git 2.55.0 as verified; the spec's 2.43
  claim and the "oldest supported git" decision (§9.5) are still open.
- **IMM-2/5:** the custom renumbering algorithm is gone; patches rely on
  `git apply --cached --unidiff-zero --recount` (proptests pass). IMM-5 found
  no bug: two distant stagings give two hunks, close ones one, as Git does.
- **IMM-3:** `Effect`/`MutationResponse` were introduced for it; the latter was
  removed again in R-1.6 (mutations now return `{effects}` + `view/render`).
- **IMM-4:** the "Read-only view" hint shows on the first attempted `:w`, not
  on E21 (no reliable autocmd for E21). Plain `:w` on a `nofile` buffer fails
  with E382 *before* `BufWriteCmd`, so a `CmdlineLeave` guard swallows it too.
  The hint wording predates text mode (R-2.6); check it still reads right.
- **IMM-6:** only the pure `resolve` + executor exist; **discard is not
  executed** (returns "Not implemented yet", belongs to R-3.6).
- **IMM-8:** `:TOhtml` needs `:packadd nvim.tohtml`, `termguicolors`, folds
  off; it ignores `hl_eol` and **skips LSP semantic-token extmarks**, so the
  colour tests need the extmark-dump fallback (§8.4 step 5). The spike now
  dumps semantic tokens (`make color-spike`).
- **R-0.2:** unmerged (`u`) entries show as `U` without hunks. Copies are not
  detected (`git diff -M`, not `-C`). `Status` gained optional `orig_path`,
  `head`, `upstream`, `ahead`, `behind`.
- **R-0.4:** symlink type changes, binary files and mode-only changes are
  whole-file only; multi-section (type-change) diffs expose no hunks.
- **R-0.5:** hunk id = first 16 hex of SHA-256 over markers+text (not `@@`
  numbers); duplicates get `-1`, `-2`. Stale ids give a warn message plus a
  fresh render, never a guess.
- **R-0.7:** the stdio loop is `tokio`, but Git still runs synchronously
  inside `spawn_blocking` (see the R-3.2 deviation below).
  A debug-only `MAGIT_LSP_TEST_DELAY_MS` hook backs the concurrency test.
- **R-3.2 deviation:** the runner deliberately keeps `std::process` inside
  `spawn_blocking` instead of porting `GitRepo` to `tokio::process`. The
  behavioural goals are met: the existing `RepoLocks` queue serialises
  mutations per repository, reads stay concurrent on other `spawn_blocking`
  threads, `window/workDoneProgress/create` + `$/progress` report mutations and
  accept cancellation, and every invocation is recorded in the bounded
  `runner::ProcessLog`. A `tokio::process` rewrite would have changed the sync
  `GitRepo` API and the whole Rust test surface for no behavioural gain.
  `CancelHandle` is thread-local per request and kills by pid (`kill -KILL`)
  because the standard library has no portable kill-by-pid and `libc` is not a
  dependency. Cancellation is consumed by the child it kills, so the failed
  mutation can still read a fresh status to report the failure.
- **R-3.3:** the process log is a read-only view over `runner::global_log()`
  (`ViewKind::ProcessLog`, scope `process-log`). `open.process-log` stays in the
  `status` scope and pushes a `Placement::Main` view through the same
  view-push/render/keymap pipeline as menus; `process-log.clear` is the only
  `process-log` command. Record nodes derive from the monotonic record id, and
  their first line records `git <args>` as `Node.copy`, so refreshes and copy
  resolve by identity rather than line shape. No new token types were added.
- **R-3.4 (core):** the bridge is server-side only. `magit-lsp
  --editor-helper <path>` connects to a per-process Unix socket, sends
  `{editId, path, kind}` and blocks for `{ok}`; `--askpass` is still a usage
  error. The worker that spawns `git commit` holds neither the sessions mutex
  nor the repo lock while Git waits: `commit.create` returns immediately,
  `common.confirm`/`common.cancel` answer the `editId`, and the worker
  re-acquires the session only to push the refreshed render (or an error
  `view/effects`). The minimal `commit.create` lands here while switches, amend
  and extend stay R-4.1; Neovim/VSCode handling of `editDocument` is a
  follow-up, so the Rust test is the client. Other kinds (`rebase-todo`,
  `merge-message`, `tag-message`) are not emitted yet.
- **R-UI-3:** the `FileChangedRO` workaround is gone from the driver; the
  client no longer sets `readonly` (Vim warns W10 on every programmatic write
  to a readonly buffer), so the harness runs Neovim untouched.
  `pending` and the live key set were not yet carried by the server protocol
  before R-UI-4; it now sends `pending` on `view/render` and `pending` +
  `liveKeys` on `view/keymap`, so `snapshot()` reports the real chord. Bug 5 did not
  reproduce (11–16 ms end to end), so `latency-menu-open` is a plain passing
  regression guard. `commit-failure-clears-progress` was added in R-UI-5.
- **R-1.1/1.2:** the status text format was kept byte-identical to the old
  Lua renderer. Position-based requests use the last *acked* tree.
- **R-1.3:** dotted token types work as-is (`@lsp.type.magit.diff.added.magit-lsp`,
  filetype `magit-lsp`). Modifier links must only add attributes (they apply
  above type groups). Added/removed use `@diff.plus`/`@diff.minus` (the old
  `diffAdded` groups do not exist in built-in schemes).
- **R-1.4:** `window/showDocument` is not used; `textDocument/definition` is
  the "visit" mechanism (deleted files return empty). `vcs/targets` is gone.
- **R-1.5:** the default visibility is fully expanded (unlike Magit, which
  collapses). No fold indicator is rendered. Laziness is proven by a global
  `diff_invocation_count` counter.
- **R-1.6:** inline diffs replace `vcs/diff`/`show_diff`; the Lua public API is
  only `setup`, `open`, `refresh`, `invoke`, `error_detail`. `src/vcs.rs` deleted.
- **R-2.1:** handlers are plain sync `fn(&mut CommandCtx)`, not `async` with a
  `Ui` handle (that arrives with R-3.1).
- **R-2.2:** `common.visit` lives in the `status` scope (its `Enter` clashed
  with `common.confirm`); vim binds `open.log` to `g l` (rule R4 forbids
  rebinding `l`); `ctrl+shift+c`-style keys are allowed (shift + letter with
  another modifier). Config reload is a 150 ms hash poll, not `notify`.
- **R-2.4:** a chord maps its full `lhs`, so Neovim waits `timeoutlen` after
  prefixes like `g`/`z`. `visit_key` (default `<CR>`) is a client-side
  exception until `common.visit` is `Implemented`. Capture mode was first tested
  with a synthetic keymap (menus now give a real capturing view).
- **R-2.5:** deny-list allow entries with reasons: `ctrl+t`, `ctrl+c`, and
  menu-scoped `ctrl+s`/`ctrl+r`. A minimal `.github/workflows/ci.yml` exists
  (no caching yet).
- **R-2.7:** pressing an option key answers "Not implemented yet … R-3.1"
  (no prompts yet); only *clearing* a preset option works. Saved args are
  in-memory per session. Menu→menu stacking is demonstrated via `?` → dispatch.
- **R-2.8:** `cargo xtask parity` is an alias (`.cargo/config.toml`) for
  `magit-lsp parity`, not a separate xtask crate. Reset's prefix is `X`
  (not `x`). In-progress actions whose key clashes with another (rebase skip/
  edit, bisect bad, merge commit/abort, cherry-pick/revert continue…) are
  omitted; branch `u/r/p/R/P` are modelled as options. 22 menus, 355 parity
  rows, ~191 Planned actions; some Magit keys are from memory and unverified.
- **R-3.1:** §4.2 lists `view/prompt` as a request, but a server request cannot
  be answered from inside a `view/key`/`view/invoke` handler: Neovim would need
  to re-enter the same client while the first request is still pending. So the
  server sends `view/prompt` as a **notification** and the client answers with a
  `view/promptRespond` **request**, which the server resolves and replies
  `null`. The wait happens on the `spawn_blocking` worker (a std mpsc sender per
  prompt id), never on a Tokio thread. `CommandCtx` keeps the sync handler shape
  and always carries `&dyn PromptUi`; `Session::{invoke,key,set_mode}` delegate
  to `NullUi` so the old signatures survive. `Session::set_mode_ui` is new so
  `view/setMode` also gets a `Ui`. `common.quit` now emits `Effect::Quit` and
  closes every view of the session instead of the roadmap message. Menu option
  prompts are `Reader`-driven: `Text/Path/Date/Number/Ref/Commit` ask for text,
  `Choice` asks with `allowCustom: true` and no items, and a cancelled prompt
  changes nothing (no render).

---

## 11. Client reviews (orchestrator, written before R-3.4b landed)

### 11.1 Neovim client (`lua/magit-lsp/`)

Sizes at review time: `init.lua` 996 lines, `menu.lua` 564, `debug.lua` 423,
`keys.lua` 326, `edit.lua` 224 (in progress), `prompt.lua` 193, `keymap.lua`
194. The client is thin in the sense that matters (no Git, tree or hunk logic;
keys, folds and clipboard are server-driven), but it has grown in structure and
in blocking behaviour. Findings, most important first; fixes are tasks
R-NV-1…R-NV-4 (Phase 3N):

1. **Every key, invoke and open blocks Neovim.** `M.press`, `M.invoke` and
   `M.open` use `buf_request_sync` plus `vim.wait` (up to 5 s, and a 5 s wait
   for the render after each invoke). While Git runs (stage, commit, fetch,
   push) the whole editor freezes; a prompt or editor-bridge request that
   arrives during the wait is handled re-entrantly inside the wait loop. This
   is the biggest user-experience risk and gets worse as Phase 4+ adds slow
   commands. → R-NV-1.
2. **Three copies of the same view plumbing.** `init.lua` (status), `menu.lua`
   and `edit.lua` each implement render application, edits, resync, the
   pending-notification queue, keymap application, press/invoke and effect
   handling. Phases 4–5 add log, revision, refs and commit-message views;
   copying the pattern each time would roughly triple the client. The status
   view also owns module-level singletons (`M.buf`, `M.view_id`, `M.version`),
   so there can only ever be one repo/status view. → R-NV-2.
3. **Test instrumentation lives in product code.** About 35 `debug_api.*` calls
   (15 in `init.lua`, 17 in `menu.lua`) and a 423-line `debug.lua` are loaded on
   every start. → R-NV-3.
4. **`init.lua` is a grab bag** (highlight links, read-only guards, text mode,
   symbols/jump, visit, error-detail buffer, setup). → R-NV-4 (split).
5. **Server binary discovery falls back to `cargo run`.** A normal install has
   no cargo project; the user sees a hang or a build. → R-NV-4.
6. **Client-side exceptions that should disappear:** the `visit_key`
   (`<CR>`) mapping and `M.visit()` (with its `wincmd p` assumption of a second
   window) exist only because `common.visit` is still `Planned`; fold-key
   mirroring is intentional (§3.9) and stays. → R-NV-4 and R-UI-7.
7. **Highlight table is duplicated by hand.** `TOKEN_LINKS` must match the
   server legend; every new token type needs a Lua edit in every client.
   Letting the server suggest a group per token type keeps clients thinner.
   → R-NV-4.
8. **Startup blocks:** `open()` polls with `vim.wait` for the first render and
   keymap (5 s each), and `setup()` always `vsplit`s (→ R-UI-7).

Things reviewed and judged fine: `keys.lua` (pure translator with unit tests),
`keymap.lua` (one atomic apply), `prompt.lua`, the read-only guards, and the
absence of any Git/tree parsing in Lua.

### 11.2 VSCode extension (`clients/vscode/`)

`extension.js` is 1197 lines in a single file next to a few small `lib/`
modules, and there is no README: `NOTES.md` only. There is no documented way to
build, launch, package or test it, and its tests cover pure helpers only (no
activation against the real server). → R-VS-1.

---

## Review

### Test-suite review (static read; tests were not run)

Scope: `tests/*.rs`, `tests/*.lua`, `tests/ui/**`, `tests/snapshots/*`,
`tests/common/mod.rs`, in-source `#[cfg(test)]` modules, `Makefile`, CI. Before
starting, run `make test && make ui-unit && make ui-test` and
`pgrep -af magit-lsp` to get a baseline. Order: do the "Do first" tasks before
the migration tasks that depend on them. File:line references are from the review
date; re-find by function name if lines moved.

- [x] **[P1] Make the editor-bridge cancel test able to fail** (`tests/editor_bridge.rs:409-477`)
  - **Problem:** `a_cancel_creates_no_commit` writes no message, then sends `common.cancel`. Git aborts on an empty message anyway, so confirm-with-empty gives identical results (no commit, index intact, error message).
  - **Why it matters:** correctness of the test (R-3.4 / §3.10 step 5: cancel replies `abort`). It currently proves nothing about cancel.
  - **Change:** write `"Should not land\n"` to `edit["path"]` before sending `common.cancel`. Assert HEAD message is still `init`, `git status --porcelain` is unchanged, and the aborted-commit `message` effect arrives. In `a_commit_is_created_from_the_typed_message` also assert `title`, `mode` and `scope` values on the `editDocument` effect (§3.10 step 3).
  - **Done when:** reverting the server's cancel to behave like confirm makes the test fail; `cargo test --test editor_bridge` passes.
  - **Approval:** the user authorizes server changes to add the documented `editDocument` metadata and correct §3.10's unsupported virtual-mode claim.

- [x] **[P1] Make the IMM-2 proptests apply selections and check the index** (`tests/git_backend.rs:754-949`)
  - **Problem:** both proptests only run `git apply --check` on each op patch against the unchanged index. They never apply ops in order (IMM-2 step 4), never compare the resulting index, and never test "stage then unstage is identity" (§8.3). `apply_check` and `apply_check_reverse` are near-identical copies.
  - **Why it matters:** correctness/coverage; a second-op offset failure after the first op is applied is invisible. Also DRY.
  - **Change:** in each property call `repo.apply_selection(&[layout], start, end, reverse)`. Build the expected index text with a small pure model in the test (original lines with the selected `+`/`-` lines applied), assert `git show :f.txt` equals it, then call the reverse selection on the fresh staged layout and assert the index equals the original. Assert `applied == false` exactly when the range contains no `+`/`-` line. Delete `apply_check_reverse` (parameterise `apply_check(dir, patch, reverse)` if one `--check` property is kept) and build the repo via `RepoFixture` (no hand-rolled `git init`).
  - **Done when:** the two proptests become one parameterised property (or share one body) that fails if `lines_patch` drops or mis-numbers a line; `cargo test --test git_backend` passes.
  - **Constraints:** keep `ProptestConfig::with_cases(64)` or lower; do not change `patch.rs`.

- [ ] **[P1] Extract one shared stdio test client** (`tests/server_async.rs:17-240`, `tests/semantic_tokens.rs:17-120`, `tests/text_mode.rs:261-380`, `tests/editor_bridge.rs:17-270`, `tests/stdout_hygiene.rs:16-110`)
  - **Problem:** five files each re-implement spawn, `Content-Length` framing, `read_frame`, `response_with_id`, `initialize`, shutdown and stderr draining.
  - **Why it matters:** DRY; every protocol test change must be made five times.
  - **Change:** add `tests/common/stdio.rs` (declared in `tests/common/mod.rs` as `pub mod stdio;`) exposing `TestServer { spawn(extra_env: &[(&str,&str)]), send, send_raw, recv, response_with_id, initialize, notifications/drain_notifications, stderr_text, finish(timeout) }`, plus `frame()` and `initialize_message()`. Port the five files to it one at a time and delete their copies. Keep per-file helpers that are truly specific (`view/prompt` helpers in `editor_bridge.rs`, token decoding in `semantic_tokens.rs`).
  - **Done when:** `grep -n "fn read_frame" tests/` finds exactly one definition; all stdio tests pass unchanged in behaviour.
  - **Constraints:** do not weaken any assertion while porting; do after the P1 cancel-test task so that file is only edited once.

- [ ] **[P1] Build the §8.1 in-process `TestSession` and migrate the session tests** (`tests/views.rs:14`, `visibility.rs:14`, `keys_protocol.rs:33`, `text_mode.rs:28`, `menu.rs:34`, `menus.rs:33`, `commands.rs:117`, `prompts.rs:35`, `process_log.rs:38`, `commit_preflight.rs:29`)
  - **Problem:** `open_status`, `keymaps()`/`session_with()`, `line_with_marker`, `node_at`, and `press`/`invoke` wrappers (with a hard-coded `version: 1`) are copied in about ten files.
  - **Why it matters:** DRY and cohesion; §8.1 specifies `TestSession` but it was never built.
  - **Change:** add `tests/common/session.rs` with `TestSession::open_status(&RepoFixture, Profile)`, `press(key)`, `press_visual(start,end,key)`, `invoke(cmd)`, `invoke_args(cmd,args)`, `ack()`, `goto_marker(path, marker)`, `goto_text(path, text)`, `text()`, `tree()`, `focused_node()`, and a `version` field that tracks the last render so keys send the real version. Migrate the listed files one per commit; delete their local copies.
  - **Done when:** `grep -c "fn open_status" tests/*.rs` is 0 for in-process files; no test sends a literal `version: 1`.
  - **Constraints:** keep behaviour of each test; use tree nodes (`line_nodes`, `marker`), never rendered text, to find cursor lines.

- [ ] **[P1] Test that the askpass secret is never logged** (`tests/editor_bridge.rs:484-525`)
  - **Problem:** §3.6/§3.10 say the secret must not be written to any log; the askpass test sends `s3cret` but never inspects the server's stderr or log file.
  - **Why it matters:** security regression guard.
  - **Change:** with the shared `TestServer` (after the stdio-client task) spawn with trace logging to stderr and a second run with `MAGIT_LSP_LOG_FILE`; after the round trip assert neither contains `s3cret`.
  - **Done when:** the test fails if the answer is passed to `tracing::debug!`.
  - **Constraints:** do not print the secret in assertion messages.

- [ ] **[P1] Add missing real-UI scenarios** (`tests/ui/scenarios/`)
  - **Problem:** §7's rule "every task that opens a new view adds a scenario" and "UI-affecting tasks are not done until a `.scn` drove the UI" is unmet for: R-3.3 process log view; R-3.1 menu option prompt; staging/unstage/fold/visual/refused-edit basics (R-1.x/R-2.4); the R-UI-5 unstaged-only confirm ("Nothing staged. Stage all changes and commit?"). `no-readonly-warning.scn` only checks no W10 and never proves an edit is refused.
  - **Why it matters:** coverage; these are user-visible behaviours only tested in Lua/Rust.
  - **Change:** add `process-log.scn` (stage a hunk, open the process log with `$`/`g $`, expect `git apply` on screen, `q` returns to status), `option-prompt.scn` (`c`, `=A`, answer the prompt, expect `--author=` on screen), `status-basics.scn` (stage `s`, unstage `u`, `Tab` fold, visual `V s`, then a native edit like `J` leaves the screen unchanged), `commit-confirm.scn` (`c c` with unstaged only, answer `n` then `y`). Under both profiles and sizes unless an opt-out is justified in a header comment.
  - **Done when:** `make ui-test` passes with the four new scenarios and writes their `.screen` files.
  - **Constraints:** no `sleep`; use `wait`. Do not add wall-clock assertions.

- [ ] **[P1] Test `view/resize` reflow** (no test exists; `src/session.rs`, `src/menu.rs`)
  - **Problem:** R-2.7 says menu layout is "re-flowed on `view/resize`"; `grep resize tests/` is empty.
  - **Why it matters:** coverage of a named acceptance criterion.
  - **Change:** in `tests/menu.rs` (or the new `TestSession`) open the commit menu at 140 cols, send the resize through the real session/server entry point to 60, and assert the resulting render's text (after applying edits) equals `menus::named("commit").render(Viewport{cols:60,..})`. Add one stdio check that the `view/resize` notification yields a `view/render` with `baseVersion`.
  - **Done when:** removing the resize handler fails the test.
  - **Constraints:** reuse the existing snapshots, add none.

- [ ] **[P2] Fix the vacuous R4 check and add rule tests** (`tests/keymap.rs:113-127`)
  - **Problem:** the forbidden set contains `"gg"` but canonical chords are space-separated (`"g g"`), so it never matches. Only whole chords are compared, so a `g g` or `ctrl+d <x>` chord is not caught. Rules R2, R3, R6 have no test.
  - **Why it matters:** the test does not enforce the rule it names.
  - **Change:** use `"g g"`; also compare `chord.steps()[0]` for single-key motions; extract the check into a function and add an injected-key negative test (like `tests/manifest.rs:216`). Add small tests: standard `common.confirm=Enter`, `common.cancel` contains `Escape`; `ctrl+c` is cancel in every non-editable scope (R6).
  - **Done when:** adding `g g` to the vim profile (temporarily) fails the suite.
  - **Constraints:** do not edit the shipped profiles.

- [ ] **[P2] Resolve the spec/impl mismatch on detail levels** (`tests/visibility.rs:232-331`, §5.1)
  - **Problem:** §5.1 defines levels 1-4 and `alt+1..4`; implementation and test have three levels (AGENTS says 1-3).
  - **Why it matters:** the test pins the implementation, not the spec.
   - **Decision:** retain four levels: section (staged/unstaged) → files → hunks → individually visible hunks. Align §5.1, shipped profile bindings and tests with this model.
  - **Done when:** spec text, profile bindings and test agree.
  - **Constraints:** no change to unrelated visibility behaviour.

- [ ] **[P2] Make `PARITY_KEYS` authoritative, not circular** (`tests/menus.rs:166-301`)
  - **Problem:** the list was written alongside the menus and "corrected here" to match them (TASKS §10: keys partly from memory). It covers about 120 of 355 rows.
  - **Why it matters:** §3.12's "every parity-list key exists in some menu" only proves self-consistency.
   - **Change:** move the list to `tests/data/magit-keys.txt` with a header naming the Magit manual version and sections; verify each key against the Magit manual. The test must fail when a menu action key is missing from the file and when a file entry has no action. Keep documented deviations (reset `X`, rebase `s`/`e`, bisect `B`) in a separate `EXCEPTIONS` table with reasons.
  - **Done when:** the data file is the single source and both directions are checked.
  - **Constraints:** do not change menu data except to fix keys proven wrong by the manual.

- [ ] **[P2] Add tests for `Unavailable` and scope-mismatch replies** (`src/session.rs:1115-1131`)
  - **Problem:** neither the `Unavailable` branch nor `"Not available in this view"` has a test.
  - **Why it matters:** R-2.1 / §3.12 acceptance paths with no coverage.
  - **Change:** in `tests/commands.rs` invoke `status.stage` against a menu view and assert the info message and no repo change. For `Unavailable`, add a test-only registry seam or a unit test of the branch with a fake command.
  - **Done when:** deleting either branch fails a test.
  - **Constraints:** no production-only test hooks beyond a `#[cfg(test)]` seam.

- [ ] **[P2] Test the acked-version rule and send real key versions** (`tests/views.rs`, `tests/navigation.rs`)
  - **Problem:** §3.1 says positional requests resolve against the last acked tree; every test queries a fresh tree, and `view/key` is always sent with `version: 1`.
  - **Why it matters:** coverage of the core consistency rule.
  - **Change:** render v2 without ack, call the definition/documentSymbol resolution and assert it uses v1; send `view/key` with a stale version and assert identity-based resolution or a message (depends on the `TestSession` task).
  - **Done when:** making positional requests use the latest tree fails the test.
  - **Constraints:** none beyond using tree nodes.

- [ ] **[P2] Test `keymapDelivery` `static`/`forwardAll` and `keyModifiers`** (`src/server.rs:316,461,1157,1267,1304`)
  - **Problem:** only `"dynamic"` is exercised (`tests/text_mode.rs:338`).
  - **Why it matters:** R-2.3 acceptance; VSCode (`static`) depends on it.
  - **Change:** stdio tests: with `static`, no `view/keymap` is pushed on `view/open` or config reload; with `forwardAll`, an unbound key replies `passThrough`. For `keyModifiers: "basic"`, test the consumer or delete the stored field.
  - **Done when:** changing the gating condition fails a test.
  - **Constraints:** use the shared `TestServer`.

- [ ] **[P2] Cover cross-scope key conflicts** (`tests/keymap.rs:206-216`)
  - **Problem:** only two commands on the same key within one scope are tested; §3.9 says "same scope chain".
  - **Why it matters:** the rule is broader than the test.
  - **Change:** add `status.stage=["Enter"]` against `common.confirm=["Enter"]` (conflict) and a scope override of a common command (not a conflict).
  - **Done when:** both cases are asserted.
  - **Constraints:** none.

- [ ] **[P2] Fill the config test gaps** (`tests/config.rs`)
  - **Problem:** untested: the `initializationOptions` layer order; `default_config_path()` under `XDG_CONFIG_HOME`; the `$VAR` form; nested includes resolving against the including file's directory; `.magit-lsp.toml` limited to keys/display; parse errors naming a line (`config.rs:143-149` checks only the file). `set_var("HOME", …)` at lines 91-92 races other tests.
  - **Why it matters:** R-2.2 / §3.18 acceptance; global env mutation is racy.
  - **Change:** add one test per gap. Replace the `set_var` calls by running the case through `ConfigService`/CLI with an explicit env, or by a `serial_test` guard.
  - **Done when:** each listed behaviour has a test; no `std::env::set_var` remains in `tests/config.rs`.
  - **Constraints:** do not change config semantics.

- [ ] **[P2] Decide the minimum git version and test it** (`README.md:56`, `.github/workflows/ci.yml`)
  - **Problem:** IMM-1 requires the unborn-HEAD tests to pass on the oldest supported git; README records only 2.55.0 and CI uses one git. TASKS §10: the `git rm --cached -f` fallback is never reached.
  - **Why it matters:** an explicit acceptance criterion with no mechanism.
   - **Decision:** Git 2.19 is a candidate minimum, pending CI validation; it is not yet the settled supported minimum. Add a CI job on that git (container) running `git_backend`, `status`, `patch_cases`. Either add a test that reaches the `git rm` fallback or delete the fallback.
  - **Done when:** README states the minimum and CI enforces it.
  - **Constraints:** none.

- [ ] **[P2] Test that a client disconnect aborts Git** (`tests/editor_bridge.rs`)
  - **Problem:** §3.10 step 7 and §8.3 name this case; no test.
  - **Why it matters:** hang risk.
  - **Change:** start a commit, receive `editDocument`, close the client's stdin/exit, and assert the Git child ends and no `git commit` is left.
  - **Done when:** the test passes and fails if the abort is removed.
  - **Constraints:** bounded wait, no fixed sleep.

- [ ] **[P2] Test the Neovim prompt mapping for every kind** (`lua/magit-lsp/prompt.lua`, `tests/test_magit_lsp.lua:1444`)
  - **Problem:** only `text` via `vim.ui.input` is covered; `confirm`, `choice` (±`allowCustom`), `multiChoice`, `secret` are not (§8.6).
  - **Why it matters:** R-3.1 acceptance.
  - **Change:** one table-driven L2 case that stubs `vim.ui.select`/`vim.ui.input`/`vim.fn.inputsecret`, sends one `view/prompt` per kind and checks the `view/promptRespond` payload shape and `cancelled`.
  - **Done when:** each kind's response shape is asserted in one case.
  - **Constraints:** do not add more Lua cases than this one.

- [ ] **[P2] Cover option clearing and saved args** (`tests/menu.rs`)
  - **Problem:** "pressing a set option again clears it" (§3.11) and `view/open` `args.saved_args` have no test; `menus.save_scope` is parsed but untested.
  - **Why it matters:** acceptance behaviour without coverage.
   - **Change:** set `=A`, press `=A` again with a `ScriptedUi` that has no reply queued, assert value `None` and no prompt. Add a `saved_args` open test. Defer `menus.save_scope` persistence and remove/defer the setting until its semantics are understood.
  - **Done when:** each case is asserted.
  - **Constraints:** none.

- [ ] **[P2] Guard VSCode keybinding drift and run node tests in `make test`** (`clients/vscode/package.json`, `Makefile`, CI)
  - **Problem:** nothing compares `package.json#contributes.keybindings` to `magit-lsp keymap --format vscode`; node tests are outside `make test`/CI (§8.7).
  - **Why it matters:** R-2.5/R-2.9 acceptance.
  - **Change:** add a check (the existing `gen-keybindings` script with `--check`, or a Rust test reading `package.json`) and a `make vscode-test` target run in CI. Overlaps R-VS-1; coordinate.
  - **Done when:** editing a profile without regenerating fails CI.
  - **Constraints:** do not hand-edit generated keybindings.

- [ ] **[P2] Replace Lua sleeps with an idle wait** (`tests/test_magit_lsp.lua:329,419,695,706,794,966,989,1008,1046,1065,1085,1142,1157,1191`; poll loops at `413-420`, `960-967`, `984-990`, `1002-1009`, `1040-1047`, `1059-1066`, `1079-1086`)
  - **Problem:** `vim.loop.sleep` and copy-pasted `for _ = 1, 40 do … sleep(50)` loops contradict "never sleep, poll `idle()`"; `type_edit` reads `v:errmsg` after a fixed sleep.
  - **Why it matters:** flakiness and DRY.
  - **Change:** add `HELPERS.wait_idle()` (poll `require('magit-lsp.debug').idle()` with `vim.wait`) and `HELPERS.wait_for(fn)`; replace every sleep/loop.
  - **Done when:** `grep -n "loop.sleep" tests/test_magit_lsp.lua` is empty; `make test-lua` passes 5 consecutive runs.
  - **Constraints:** do not change assertions.

- [ ] **[P2] Make `type_edit` assert what IMM-4 says** (`tests/test_magit_lsp.lua:324-341`, `621-657`)
  - **Problem:** it passes both when the key succeeds and when it fails with E21, so a key that silently edits nothing passes. `dd`/`x` from IMM-4 are skipped because they are bound.
  - **Why it matters:** weak assertion.
  - **Change:** assert the buffer is unchanged, `modified=false`, and for keys unbound in the active profile that E21 occurred. Note the `dd`/`x` deviation in the test header.
  - **Done when:** making the buffer modifiable fails the test.
  - **Constraints:** no new Lua cases.

- [ ] **[P2] Force overlap in the mutation-ordering test** (`tests/server_async.rs:373-410`, `325-370`)
  - **Problem:** `mutations_for_one_repository_apply_in_request_order` passes even without serialisation because both mutations are fast. `a_slow_mutation_does_not_delay_a_later_read` exists only under `cfg(debug_assertions)` via `MAGIT_LSP_TEST_DELAY_MS`.
  - **Why it matters:** R-3.2 "serialises mutations per repository" is not truly tested.
  - **Change:** reuse the fake-git `add` hook (shared fixture, see C9 task) so the first mutation blocks until released; send the second; assert the second's `$/progress` `begin` follows the first's `end`. Switch the read-concurrency test to the same hook so it also runs in `--release`.
  - **Done when:** removing the repo lock fails the ordering test.
  - **Constraints:** bounded waits only.

- [ ] **[P2] Prune cross-layer duplicate tests** (see list)
  - **Problem:** the same behaviour is asserted in several layers: ~12 Lua staging/untracked cases (`tests/test_magit_lsp.lua:395-500, 800-872`) duplicate `views.rs`/`git_backend.rs`/`status.rs`; the 23-line status table in Lua `800-829` duplicates `status_render.rs:102`; `tests/keys.rs` duplicates `src/keys.rs:392-420`; `tests/manifest.rs:86-127` duplicates `src/keys.rs:423-485`; `src/runner.rs:392-420` duplicates `tests/runner.rs:131-169`; `unbind_removes_a_profile_binding` exists in `config.rs:172` and `keymap.rs:219`; Planned-action checks exist in `commands.rs:203`, `menu.rs:296`, `menus.rs:110`.
  - **Why it matters:** §0/§8 target ~80/15/5 split; duplicates raise churn.
  - **Change:** keep one Lua stage round trip (`pressing s…`, `visual line range`), the Lua notify case for planned actions, and delete the rest of the listed duplicates. Delete `tests/keys.rs`, the two `*_keys_follow_the_section_3_8_table` tests and one of each duplicated pair above. For Rust/Lua translator drift, share one `tests/data/keys.json` read by `src/keys.rs` tests and `test_keymap.lua`.
  - **Done when:** each behaviour has one primary owner; `make test` passes; Lua case count drops by at least 12.
  - **Constraints:** do not remove the only test of any behaviour; do after the `TestSession` task.

- [ ] **[P2] Reduce snapshot sprawl and use one snapshot mechanism** (`tests/manifest.rs:22-43`, `tests/parity.rs:119-164`, `tests/menu.rs:146-158`, `tests/menus.rs:527-556`, `tests/snapshots/`)
  - **Problem:** three snapshot mechanisms (`expect_file!`, a custom `UPDATE_SNAPSHOTS` helper twice). `parity` is snapshotted three times (README block, `parity.md` with 355 rows, CLI output). 12 menu layout snapshots pin incidental spacing. Manifest snapshots in four formats pin docs/statuses that change every phase.
  - **Why it matters:** brittle, noisy diffs on every Planned→Implemented change.
  - **Change:** use `expect_file!` everywhere and delete both custom helpers. Delete `tests/snapshots/parity.md` and its test; keep the README-block check. Keep layout snapshots for the commit menu at 60 and 140 only; for rebase/push/diff assert width invariants (no line wider than `cols`, fewer columns at smaller widths, every Planned action has a `magit.menu.planned` token). Keep `manifest-*.json` and `keybindings-*.json` (R-2.5 requires a CI snapshot); drop the markdown and `.neo.json` snapshots.
  - **Done when:** `tests/snapshots/` has ≤ 6 files and one update command (`UPDATE_EXPECT=1`); documented in `README.md`/`AGENTS.md`.
  - **Constraints:** keep the R-2.5 manifest snapshot.

- [ ] **[P2] Replace magic counts and interim-state asserts** (`tests/commands.rs:199`, `tests/menus.rs:162`, `tests/parity.rs:82`, `tests/menu.rs:716-742`, `tests/dispatch.rs:583-664`)
  - **Problem:** `== 32`, `> 100`, `> 150` thresholds; asserts that commit keys `e`/`f`/`a` are `Planned R-4.1` (breaks when R-4.1 lands); five `execute_discard_*` tests pin "Not implemented yet" for R-3.6; `needs_confirm: true` appears in every discard row with no `false` case.
  - **Why it matters:** brittle and tests the temporary state, not the spec.
  - **Change:** use relational checks (`planned + implemented == total`, "at least one planned action exists"); keep a single discard-deferred test marked `// remove with R-3.6`; drop `needs_confirm` or add a row where it is false.
  - **Done when:** landing R-4.1 or R-3.6 breaks at most one clearly marked test.
  - **Constraints:** none.

- [ ] **[P2] Tighten loose or tautological assertions** (`tests/git_backend.rs:55-73,162-212,633-716`, `src/patch.rs:714-721`, `tests/manifest.rs:198-213`, `tests/keys_protocol.rs:416`, `tests/text_mode.rs:226`, `tests/parity.rs:16-83`, `tests/commit_preflight.rs:74`)
  - **Problem:** `contains` checks where the exact diff is known (and `stage_hunk_stages_entire_file` is misnamed, the fixture has one hunk); `full_hunk_reconstruction_is_valid` checks three substrings; `allow_entries_are_scoped_with_reasons` asserts constants against themselves; stray final asserts that repeat earlier ones; `every_menu_item_has_a_parity_row` uses `ends_with(" {key}")` so `a` matches `g a`, and `every_action_row_matches_the_registry…` never reads rows; `_tmp` is used.
  - **Why it matters:** weak or vacuous asserts.
  - **Change:** `assert_eq!` against the full expected `git diff --cached -U3`; rename `stage_hunk_stages_entire_file` to `stage_hunk_stages_the_hunk`; check the reconstructed hunk via `apply_check`; replace the allow-list test with "every `WORKBENCH_ALLOW` key is in the deny list and has a non-empty reason"; delete the stray asserts; compare exact `(menu, key)` tuples in parity; rename the registry-row test.
  - **Done when:** each listed assert is exact or removed.
  - **Constraints:** do not delete the behaviour coverage.

- [ ] **[P2] Remove process-global env mutation from tests** (`tests/config.rs:91-92`, `tests/runner.rs:37-82`, `tests/process_log.rs:22`)
  - **Problem:** `set_var("HOME"/…)` and `set_var("PATH", …)` in a multithreaded test binary race with other tests; the process log is global.
  - **Why it matters:** flakiness.
  - **Change:** run those cases through the CLI/stdio server with an explicit env, or guard with `serial_test`. For the process log, filter by a unique `cwd` (as `runner.rs` does) and drop the lock.
  - **Done when:** `grep -rn "set_var" tests/` only matches `tests/common/mod.rs`'s once-guarded init.
  - **Constraints:** none.

- [ ] **[P3] Make UI latency budgets informational and avoid blind key counts** (`tests/ui/scenarios/latency-menu-open.scn:12-32`)
  - **Problem:** hard wall-clock budgets (`<= 100ms/80ms/400ms`) fail on slow CI; `j ×10 Right ×4` finds a diff line by count.
  - **Why it matters:** flakiness; fragile to fixture changes.
  - **Change:** report latency without failing unless `UI_STRICT_LATENCY=1`; add a `goto "+line3 CHANGED"` DSL step (or use a search key) instead of counted motions.
  - **Done when:** `make ui-test` passes on a loaded machine; the scenario still reports numbers.
  - **Constraints:** keep `ui-unit` tests for the DSL green.

- [ ] **[P3] Stop deriving cursor lines from rendered text in protocol/Lua tests** (`tests/server_async.rs:495-501,602-610`, `tests/test_magit_lsp.lua:150-169`)
  - **Problem:** `trim_start().starts_with('+')` and `line:match('^  %S ')`/`startswith('    @@')` parse rendered text (AGENTS convention).
  - **Why it matters:** convention; the helpers break on any format change.
  - **Change:** request `textDocument/documentSymbol` and use the hunk/file symbol `selectionRange`, or `M.jump`.
  - **Done when:** no helper pattern-matches line shapes.
  - **Constraints:** keep helper names so call sites do not change.

- [ ] **[P3] Housekeeping** (`tests/stdout_hygiene.rs:210-257`, `.github/workflows/ci.yml:26-27`, `tests/runner.rs:29-93` + `tests/server_async.rs:835-881`, `tests/common/mod.rs:252-295`)
  - **Problem:** (a) a hand-rolled source scanner for `println!`; (b) the manifest test runs twice in CI; (c) fake-git fixture duplicated; (d) five repo constructors (`init_repo`, `init_long_repo`, `init_two_file_repo`, `init_40_line_repo`, `init_repo_40`) and doubled helper names (`make_standard_changes`/`standard_changes`, `make_two_hunks`/`two_hunk_changes`) beside `RepoFixture`, plus hand-rolled `git init` in `errors.rs:133-139,270-282,297-310` and `git_backend.rs:324-329`; `(1..=40).map(|i| format!("line{i}"))` inlined about eight times.
  - **Why it matters:** DRY/cleanliness.
  - **Change:** (a) add `#![deny(clippy::print_stdout)]` and delete the scanner test (keep the two stdio tests); (b) delete the extra CI step; (c) move one `FakeGit` to `tests/common/fake_git.rs`; (d) keep `RepoFixture` + `init_repo()`, add `lines(n).with(idx, text)`, convert the hand-rolled setups to the builder, delete the wrappers.
  - **Done when:** each sub-item is done; `make test` passes.
  - **Constraints:** do after the `TestSession` and stdio-client tasks.

- [ ] **[P3] Remaining small coverage gaps**
  - **Problem:** (a) IMM-3: exit code is 128 in every row of `errors.rs:52-119`; no real `non-fast-forward` via a local bare remote; no `processLogId`; (b) §3.13 `diff.context` parsed but no test that `-U<n>` reaches both diffs; (c) §8.4 colour checks beyond links (section ≠ file ≠ hunk header ≠ hash; `menu.planned` dimmer than `menu.label`) are absent; (d) R-UI-5 progress ending on a failed commit is only asserted at L3; (e) `.cargo/config.toml` alias `cargo xtask parity` is untested; (f) `make test` omits `ui-unit` and the node tests (document it).
  - **Why it matters:** acceptance details without tests.
  - **Change:** add one test per item (reuse existing fixtures; (c) goes into the existing colour-scheme Lua case, (d) into `server_async.rs` with a failing `pre-commit` hook).
  - **Done when:** each item has a test or a recorded decision to skip.
  - **Constraints:** do not add Lua cases beyond (c).

### Test-review summary (for the human)

Counts: 7 review P1, 20 review P2, 4 review P3 tasks above (plus one separate
P1 harness bugfix). Decisions recorded: four detail levels (§5.1); Git 2.19 is
a candidate minimum pending CI; parity keys must be verified against the Magit
manual; defer `menus.save_scope` persistence and revisit its semantics before
adding a persistence setting.

### 11.3 UI harness review and bugfix

- [ ] **[P1] Diagnose Neovim harness startup/socket failure** (`scripts/ui`, `scripts/ui_lib/`)
  - **Problem:** Neovim does not open the expected `build/ui/runs/.../nvim.sock` socket.
  - **Why it matters:** the real-UI harness cannot reliably start or connect to its child Neovim.
  - **Change:** diagnose process startup, socket path creation, permissions and connection ordering. Keep harness startup/socket diagnosis separate from scenario behavior: a scenario assertion failure is not evidence of socket failure, and a socket failure is not a scenario behavior regression.
  - **Done when:** the harness reports a clear startup/socket outcome and can connect to the expected socket; scenario behavior is independently tested by `make ui-test`.
