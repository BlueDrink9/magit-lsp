# Architecture and design

This document is the maintained reference for project decisions, glossary,
completed correctness contracts and architecture design.

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
| VCS boundary | Keep presentation and standard-LSP transport VCS-neutral so future jj support requires effectively zero Plain-client changes. Keep Git discovery, mutation validation, commit-file paths, helper lifecycle and completion lookup in backend/workflow providers. Do not add a jj backend, lowest-common-denominator VCS hierarchy, or speculative wire protocol |
| Diff/patch | Git computes diffs; we parse its output and build patches for `git apply` (this is what Magit does, §3.13) |
| Rendering | Server re-renders fully (as Magit does) and sends line-range edits, so clients stay stable (§3.3) |
| Tests | Mostly Rust, in-process, asserting buffer text and repo state. Neovim tests are kept but few (§8) |
| Detail visibility | Four levels: section (staged/unstaged) → files → hunks → individually visible hunks; profile bindings and tests must match (§5.1) |
| Menu saved arguments | Defer `menus.save_scope` persistence; remove/defer the setting until semantics are understood |
| Magit parity keys | Verify keys against the Magit manual, not memory |
| Recommended Git version | Git 2.30.8 is the tested recommendation, not a hard minimum; older versions are not blocked but may have feature failures |
| Picker (T008) | Server supplies typed labels and stable IDs for single/multi selection; clients return IDs. Clients never filter, parse or resolve Git state. A native widget may filter server-supplied labels. |
| Tests | Feature Rust state tests and conformance scenarios; native Neovim only for new visible behavior, VSCode only for changed declared capabilities or client files. |
| Generated files | Ship only files clients need; mark generated and enforce drift checks. |
| Capability resolution with quirks | Resolve declarations, then client quirks, then user configuration (T005). |
| Rich protocol | The rich protocol stays; v1 preview permits additive changes only. |
| Plain presentation (target) | Standard-LSP presentation is VCS-neutral. T008 serving/stacking and parameterized stable-ID picking; T009 provider-file edits; T010 semantic dispatcher and fake-provider independence test. Intended architecture, not current behavior. |
| Plain support (target) | Generic LSP client selection, sequential URI-keyed sync, safe repository resolution, validated targets and bounded capabilities; deferred/out-of-scope work remains in the numbered queue. |

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
| **Editor bridge** | Git's helper waits until the client confirms or closes Git's real `COMMIT_EDITMSG`, or explicitly aborts (§3.10). |
| **Process log** | A view listing every Git invocation with arguments, exit code, duration and output |
| **Text mode** | A view mode with no ordinary Magit key capture: Neovim retains its existing editable buffer behavior; VSCode browses its read-only virtual document with native editor keys and queued refreshes (§3.14) |
| **Semantic token** | LSP's standard way for a server to label ranges of text with a *type* (and modifiers) so the client colours them using the user's theme |
| **Parity** | Having the same operation available as Magit |

---

## 2. Completed correctness contracts

Completed hands-on fixes, retained here for their behavioural contracts.
These contracts remain canonical; open evidence gaps and follow-ups are tracked
in [the implementation queue](../TASKS.md). Historical completion marks are not proof that every original test exists.

### Completed correctness contracts (immutable completion record: `da4d422:TASKS.md`)

- **IMM-1:** Unstage works in unborn repositories and rename paths. Tests: `tests/dispatch.rs`; oldest-Git/fallback evidence gap remains.
- **IMM-2:** Normalize tree-based inclusive selections; retain only changes and safely reverse-apply. Tests: patch/dispatch suites; randomized property evidence per [§8.3](../TASKS.md#83-other-rust-layers).
- **IMM-3:** Git failures become readable message effects; protocol faults remain RPC errors. Tests: `tests/errors.rs`; mapped failures enforced, broader UI evidence retained in archive.
- **IMM-4:** Neovim status buffer is render-only except text mode; writes blocked. Tests: `tests/editor_bridge.rs`; native rules and evidence remain in immutable record.
- **IMM-5:** Git diff context determines hunk merge; patch builder does not compute diffs. Tests: patch fixtures and cached diff parity.
- **IMM-6:** Dispatch uses smallest tree node or explicit selection. Tests: `tests/dispatch.rs`; discard execution/wiring remains R-3.6.
- **IMM-7:** Shared deterministic test harness in `tests/common/session.rs`. Tests: harness consumers; completion record retained.
- **IMM-8:** Historical `:TOhtml` spike only; superseded by T028. No rerun; do not treat spike as production evidence.
- **IMM-9:** `AGENTS.md` links roadmap; decisions and working rules have canonical homes. Tests: documentation contract; no runtime test.

---
