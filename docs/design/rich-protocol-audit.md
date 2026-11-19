# Rich protocol audit (T112)

Scope: facts about the rich protocol at a single committed baseline. This is
an audit, not a decision record and not a cleanup ticket. No source file was
changed.

- **Baseline:** `f15c026b2dcbb11397d904572b8a28004a3f8167` (`f15c026`, "Sign off T022 plain gate").
- **Audit date:** 2026-11-19.
- **Method:** read the baseline blobs (`git show f15c026:<path>`) for every file.
  `src/commands.rs`, `src/git.rs`, `src/menus/branch.rs`,
  `clients/vscode/lib/requests.js`, `TASKS.md` and `tests/commands/more.rs`
  were dirty in the worktree during the audit; all claims below use their
  committed content at `f15c026`, not the dirty worktree. Other referenced files were
  unchanged by the dirty set.
- **Binding constraints:** pre-1.0 breaking freedom, server-owned tree/text,
  negotiated capabilities and boundary input validation remain assumptions.
  A "wire change: yes" below does not imply compatibility work.
- **Evidence convention:** `path:line` means the committed blob at `f15c026`.
  `docs/` references use the same base.

## Status summary

| # | Concern | Status | Wire change | Effort |
| --- | --- | --- | --- | --- |
| 1 | Keymap reuses render version; draft keymaps use 0 | Real (ambiguity) | No if documented; yes if removed | S |
| 2 | `view/render` sends zero or one edit; multi-edit rules unwritten | Real (documentation) | No | S |
| 3 | Draft `runId` keymaps may be dead after real-file cutover | Not real | No | None |
| 4 | One acknowledgement snapshot vs. several outstanding renders is unwritten | Real (documentation) | No | S |
| 5 | No protocol version check or readable mismatch error | Real | Yes | M |
| 6 | `keyModifiers` is read and logged but inert | Not real | No | None |
| 7 | VS Code cannot replay non-printable/non-clipboard `passThrough` | Real | No | S–M |
| 8 | Static clients learn mode changes only from `view/render` mode | Real (narrow: menu entry) | No | S |
| 9 | Standard reads use the acked tree (current before first ack); rich commands bind an exact version with ack exceptions | Real (documentation) | No | S |
| 10 | Historical prompt wording vs maintained docs and transport | Real (documentation drift) | No | S |
| 11 | VS Code declares layouts it does not honor | Not real (fixed by T004) | No | None |
| W1 | `PROTOCOL` omits `view/invoke.version` | Real (documentation) | No | S |
| W2 | `PROTOCOL` lists nonexistent keymap binding fields | Real (documentation) | No | S |
| W3 | Mode-only render reuses the current version | Real (minor ambiguity) | No | S |

Statuses use only **real**, **not real** and **unknown**. No item below is
**unknown**: each has direct code or documentation evidence. "Not real" means
the concern does not describe baseline behavior; it does not mean the historical
report was dishonest.

## Findings

### 1. Keymap version reuses the render version; draft keymaps use 0

- **Status:** Real (semantic ambiguity, not a functional defect).
- **Evidence:** `ViewKeymapParams.version` is documented only as `version`
  (`src/protocol.rs:359-376`, especially `:365`). `Session::keymap_notice`
  fills it from `view.version` (`src/session.rs:794-823`, especially `:812`).
  Draft/editor keymaps use `version: 0` with the `runId` as the view id
  (`src/server/view_execution.rs:577-593`, especially `:584-586`). The render
  version is a separate counter (`src/session.rs:369-371,418-461`). Entering
  text mode can change `mode` without advancing that counter
  (`src/session/commands.rs:312-330`).
- **Proposal:** Document that `view/keymap.version` is the render generation the
  bindings belong to for a real view, that it is advisory (never ackable), and
  that `0` means a non-view draft/edit keymap keyed by `runId`. Do not use it
  for acknowledgement or revision comparison.
- **Wire change:** No if documented; yes only if the field is renamed or
  removed (both allowed pre-1.0).
- **Risk:** Low. Clients already ignore it for acknowledgement: Neovim stores
  the notice and applies bindings (`lua/magit-lsp/view.lua:435-496`), VS Code is
  static and never receives it (`clients/vscode/lib/session.js:64-66`).
- **Effort:** S.

### 2. `view/render` sends zero or one edit; multi-edit rules are unwritten

- **Status:** Real (documentation gap). The server never sends more than one edit.
- **Evidence:** `diff_text::line_edits` always returns either an empty vector or
  exactly one `LineEdit` covering the common prefix/suffix replacement
  (`src/diff_text.rs:9-38`). The session emits `text` or `edits`, never both
  (`src/session.rs:436-446`). Both clients already implement multi-edit
  application: Neovim sorts by descending start line and splices
  (`lua/magit-lsp/buffer.lua:121-128`); VS Code does the same
  (`clients/vscode/lib/render.js:44-60`). The protocol type permits
  `Vec<LineEdit>` (`src/protocol.rs:195-217`) without stating ordering,
  overlap or atomicity rules.
- **Proposal:** State the contract explicitly. Narrowest option: the server
  currently emits at most one edit, and clients must apply multiple
  non-overlapping edits bottom-up (descending `startLine`, `endLine` exclusive)
  if a future server sends them. Keep the type as is.
- **Wire change:** No.
- **Risk:** Low. A future server-side optimization could emit overlapping or
  ascending edits that a client without the stated rule applies incorrectly;
  today no such server exists.
- **Effort:** S.

### 3. Draft `runId` keymaps may be dead after the real-file cutover

- **Status:** Not real at this baseline.
- **Evidence:** `EditDocumentMode` has exactly one variant, `RealFile`
  (`src/protocol.rs:66-72`), so the "cutover" is already the only mode.
  `edit_keymap` produces a keymap keyed by `run_id` (`edit_id`) for
  registry-scoped commit-message kinds (`src/server/view_execution.rs:577-593`).
  It is delivered only to dynamic-keymap clients in the same path that delivers
  render/edit notifications (`src/server/view_execution.rs:550-558`; gate at
  `src/server.rs:764-768`). Neovim registers the draft under
  `view_key(client_id, runId)` and flushes pending keymaps on open
  (`lua/magit-lsp/view.lua:734-745,781-785`; `lua/magit-lsp/edit.lua:240-291`).
  VS Code declares `keys: 'static'` (`clients/vscode/lib/session.js:64-66`) and
  has no `view/keymap` handler at all (`clients/vscode/lib/*.js`), so it is not
  expected to consume the notice.
- **Proposal:** No code change. Optionally name the artifact an "edit keymap"
  in docs to avoid implying it is a normal view keymap; item 1 covers its
  version semantics.
- **Wire change:** No.
- **Risk:** None today. If a future static client wants server-owned draft keys,
  that is new work, not a dead path.
- **Effort:** None.

### 4. One acknowledgement snapshot vs. several outstanding renders is not written

- **Status:** Real (documentation gap; the implementation retains exactly one
  acknowledgement snapshot, but that does not bound outstanding deliveries).
- **Evidence:** `OpenView` stores one `acked: Option<Tree>`, one
  `request_tree`, and one `acked_version: Option<u64>`
  (`src/session.rs:201-210,330-339,369-382`). `OpenView::render_tree_with_delivery`
  increments `version` and returns the render without waiting for an ack
  (`src/session.rs:418-460`); nothing tracks or limits pending deliveries. The
  retained acknowledgement only selects the edit base: edits are emitted when
  `self.acked_version == Some(old_version)`, otherwise the render carries full
  `text` and no edits (`:436-446`, `:448-455`). `Session::ack` records the ack
  only when `view.version == version`; a stale or future ack is silently
  ignored and still returns `Ok(())` (`src/session.rs:991-1003`). Clients ack
  the render they applied (`lua/magit-lsp/view.lua:394-401`;
  `clients/vscode/lib/requests.js:147-155`). The historical BUG-READ-1 wording
  about "several unacked versions" (`TASKS_PART_SEVEN.md:479`) is still not a
  multi-version store — no code path retains more than the latest acknowledged
  tree — but it is not accurate to conclude that only one version can be
  delivered and unacked at a time.
- **Sequence (baseline, one view):** the client acks version N
  (`acked_version = Some(N)`); the server renders N+1 with edits based on N and
  `version` becomes N+1 while `acked_version` stays `Some(N)`; the server
  renders N+2 before any ack, `acked_version` is still `Some(N)`, so
  `base_version` is `None` and N+2 carries full text (`:438-455`); an ack of
  N+1 now finds `view.version == N+2` and is ignored (`:996-1000`); an ack of
  N+2 becomes the retained snapshot. Each delivery was a normal `view/render`;
  no error or deferral occurred.
- **Proposal:** Document the actual contract: a view retains one acknowledged
  tree/version (the edit base for later diffs and the source for acknowledged
  positional reads), not a single in-flight delivery slot; successive renders
  may be delivered before any ack, and any render whose base is not the retained
  acknowledged version carries full text. Acks are valid only for the current
  version, stale acks are idempotent no-ops, and clients request `view/resync`
  after a lost update rather than acking older versions. The proposed rule
  "each view has at most one outstanding delivered version" is not baseline
  behavior; adopting it would require server-side flow control (deferring or
  rejecting renders until ack) and is a behavior change needing implementation,
  not an existing rule to document.
- **Wire change:** No for documenting baseline semantics. Single-outstanding
  delivery would be a behavior change, not a wire shape change.
- **Risk:** Low for the documentation correction. The silent stale-ack no-op
  can hide a client bug, and an out-of-order ack forces the next render to full
  text; both shipped clients ack the applied latest version. Flow control would
  carry its own concurrency risk and design work.
- **Effort:** S (documentation). Adding flow control would be M and needs its
  own design.

### 5. No protocol version check; mismatches have no readable error

- **Status:** Real.
- **Evidence:** The rich declaration validates only `v == 1` through a custom
  deserializer (`src/presentation/mod.rs:37-77`, especially `:58-60`). The
  initialize result returns standard capabilities plus
  `serverInfo {name, version}` and negotiated `experimental.magit.effective`;
  it carries no protocol version or minimum-client-version field
  (`src/server/requests.rs:256-297`). A malformed or future version is rejected
  as a capability-declaration error, and an older client that omits fields gets
  the same generic validation path (`src/presentation/mod.rs:42-43`).
- **Proposal:** Add an explicit handshake: server reports its rich protocol
  version in the initialize result; client reports the minimum it needs; either
  side emits a readable `Effect::Message`/error when they do not match. Pre-1.0
  may make this a breaking addition.
- **Wire change:** Yes (initialize result and/or capability declaration fields).
- **Risk:** Medium. The change is small, but every rich client must report and
  read it; a wrong default recreates the silent mismatch.
- **Effort:** M.

### 6. `keyModifiers` is read and logged but does not change behavior

- **Status:** Not real. It changes both delivered bindings and server-side key
  resolution.
- **Evidence:** Initialization parses the rich declaration into
  `RichCapabilities.modifiers` (`src/presentation/mod.rs:37-51,54-77`), stores
  it on `ClientCaps` (`src/server.rs:122-141`) and logs it
  (`src/server/requests.rs:204-213,251`). `Session::set_rich_capabilities`
  keeps it (`src/session.rs:744-746`, called from
  `src/server/view_execution.rs:52-63`). `keymap_notice` applies
  `with_basic_modifiers` before delivery (`src/session.rs:794-802`), and key
  resolution applies it before matching (`src/session.rs:1244-1250,1301-1307`).
  `Keymap::with_basic_modifiers` rewrites the delivered `keys` list and
  `Keymap::lookup` skips non-basic chords (`src/keymap.rs:224-256,287-297`).
- **Proposal:** None. Documentation should describe the basic-modifier filtering
  (including the lone `shift+Tab` exception) as actual behavior. Do not add a
  second mechanism.
- **Wire change:** No.
- **Risk:** None.
- **Effort:** None.

### 7. VS Code cannot replay a `passThrough` key that is not printable or a clipboard shortcut

- **Status:** Real.
- **Evidence:** VS Code replays only `ctrl+c`, `ctrl+x`, `ctrl+v`, `Space`, and
  single-character strings; every other key logs
  `passThrough: cannot replay ... from a static binding`
  (`clients/vscode/lib/effects.js:7-35`). The server can emit arbitrary
  canonical chords as `Effect::PassThrough { key }`
  (`src/protocol.rs:29-30`; resolution at `src/session.rs:1343-1346`).
  VS Code declares `modifiers: 'full'` (`clients/vscode/lib/session.js:64-66`),
  so the server may also offer non-basic chords it cannot replay.
- **Proposal:** Choose one deliberately: (a) map common non-printable keys
  (`Enter`, `Escape`, `Tab`, arrows, deletion) to VS Code commands; (b) declare
  `modifiers: 'basic'` for VS Code so non-basic chords are not delivered; or
  (c) document the limitation and keep printable/clipboard fallback. Option (b)
  is the narrowest honest declaration, but it also removes bindings; (a) is
  behavior work; (c) is documentation only.
- **Wire change:** No for (a)/(c); (b) changes a declared capability value, not
  the wire shape.
- **Risk:** Medium if a captured key silently does nothing; the current log is
  developer-facing, not user-facing.
- **Effort:** S–M.

### 8. Static clients learn about a mode change only through render-derived context

- **Status:** Real, but narrow: invoke, key and `view/setMode` work; the
  dispatch-menu entry targets the menu itself.
- **Evidence:** VS Code is the static client (`clients/vscode/lib/session.js:64-66`).
  It derives `magit.textMode`/`magit.capture` from the render's `mode`
  (`clients/vscode/lib/context.js:19-38`), stores `state.mode` on every render
  (`clients/vscode/lib/requests.js:125-127`) and reapplies context only for the
  active view (`clients/vscode/lib/requests.js:141-143`,
  `clients/vscode/lib/views.js:164-177`). Mode changes go through one command
  (`common.toggle-text-mode`, `src/commands.rs:883-890`, handler at `f15c026`
  `src/commands.rs:1851-1862`) and produce a render with the new `mode`
  (`src/session/commands.rs:312-330`; keymap notice at `:663-667`).
  `view/invoke`, `view/key` and `view/setMode` all reach that command
  (`src/session.rs:1048-1088,1191-1354,2253-2294`). The dispatch menu lists
  `common.toggle-text-mode` (`src/menus/dispatch.rs:54-60`). Because
  `Scope::Common` matches every scope (`src/commands.rs:67-70`), the menu action
  runs against the menu view itself (`src/session/menu.rs:454-480`) rather than
  the ancestor that owns the document. VS Code presents menus as a QuickPick
  (`clients/vscode/lib/menus.js:22-41`), so no text buffer and no caller context
  changes.
- **Proposal:** Route text-mode changes and other document-only commands from a
  menu to the nearest ancestor with a document (same idea as
  `matching_ancestor`, `src/session.rs:2030-2041`), or omit
  `common.toggle-text-mode` from the dispatch menu. The invoke/key/setMode
  paths need documentation, not code.
- **Wire change:** No (server-side routing; effects unchanged).
- **Risk:** Medium-low. A user invoking "Toggle text mode" from the dispatch
  menu currently changes the wrong view (or an unshowable menu mode).
- **Effort:** S.

### 9. Standard LSP reads and rich command versioning are different policies

- **Status:** Real (documentation). Both behaviors are implemented and
  coherent, but they are different policies and the maintained protocol
  reference states neither.
- **Evidence (standard LSP reads):** `tree_for_positional_request`
  (`src/server.rs:1533-1536`) calls `Shared::view_tree_for_uri`
  (`:800-815`), which finds the view published for the URI and returns
  `PublishedViews::acked_tree_for_uri` (`src/session.rs:172-179`). That tree is
  fixed when the view is published (`src/session.rs:97-125`, especially
  `:111-112`): the last acknowledged tree, or the current `view.tree` when no
  acknowledgement exists yet, and `None` in text mode. The standard providers
  use it for `textDocument/documentSymbol`, `definition`, `documentLink`,
  `foldingRange` and `semanticTokens/full`
  (`src/server/requests.rs:334-355,361-387,388-409,411-422,424-450`), and each
  returns `null` in text mode before resolving a tree (`:337,365,391,414,427`).
  Plain provider documents use their own plain-provider snapshot instead. The
  docstring at `src/server.rs:800-803` says positional requests cannot refer to
  a render the client has not applied, which overstates the pre-first-ack
  current-tree fallback.
- **Evidence (rich invoke/key commands):** `view/invoke` and `view/key` supply
  `action_target: None` with the client's version (`src/session.rs:1068-1081`,
  `:1326-1342`) and share `Session::run_versioned_command`
  (`src/session/commands.rs:17-90`). For positional commands that consume
  cursor/selection or require an acknowledgement, outside text mode
  (`:38-67`), resolution accepts the exact acknowledged version (`:69`), the
  exact current version for commands that do not require an acknowledgement
  (`:76`; not limited to the first-ack window), and, for a request carrying a
  validated offer with no acknowledgement yet, the exact current version
  (`:70-75`). `select.toggle-mark` and a `common.confirm` with a commit
  selection require the acknowledgement (`:40-41`), so a validated action offer
  is the one exception before the first ack. Anything else produces the
  readable stale-view warning (`:79-89`). The `action_target` branch in
  `Session::dispatch` (`src/session.rs:1117-1169`) is a different path:
  `action_target` is `Some` only for Plain provider content targets
  (`src/server/plain_actions.rs:980-983`), while rich invoke and key pass
  `None`.
- **Proposal:** Document the two policies separately instead of one
  "positional" rule. Standard reads: last acknowledged tree, current tree
  before the first acknowledgement, `null` in text mode. Rich commands: exact
  acknowledged or exact current version under the acknowledgment and
  validated-offer rules above. In the maintained reference, add `version?` to
  the `view/invoke` row and a standard-read resolution note to §4.1 (the
  maintained gap tracked as W1). `docs/CLIENT_SPEC.md:63,75` and
  `docs/ARCHITECTURE.md:102-104,130-135` already describe parts of both
  policies, but `ARCHITECTURE §3.1` omits the pre-ack fallback and
  `docs/PROTOCOL.md:16` omits the invoke version. Keep the behavior.
- **Wire change:** No.
- **Risk:** Low. A new client that implements only "last acknowledged tree"
  would mishandle pre-first-ack reads, and a protocol reader would not learn
  that standard reads and rich commands resolve differently.
- **Effort:** S.

### 10. Historical prompt wording vs maintained docs and current transport

- **Status:** Real (documentation drift). The implementation and
  `CLIENT_SPEC.md` agree; `ARCHITECTURE.md` §3.6 and `PROTOCOL.md` §4 still say
  request/response.
- **Evidence:** The wire is a server notification plus a separate client
  request: `VIEW_PROMPT = "view/prompt"` is sent with
  `Self::notify(VIEW_PROMPT, &prompt)` (`src/protocol.rs:422-425` and
  `src/presentation/custom.rs:170-184`), while `VIEW_PROMPT_RESPOND =
  "view/promptRespond"` is a client request (`src/protocol.rs:507-521`).
  `PromptBridge::ask` sends the notification and blocks on a channel; the
  response arrives through `PromptBridge::respond`
  (`src/server/ui.rs:28-91,114-120`). `CLIENT_SPEC.md:53-54` describes exactly
  this notification/respond split. `ARCHITECTURE.md:180-181` says the server
  sends "`view/prompt` *request* and waits for the response", and
  `PROTOCOL.md:30` labels `view/prompt` "(request)" returning
  `{cancelled, value?}`. The historical TASKS wording is a premise, not current
  evidence; the maintained docs are the defect.
- **Proposal:** T113 writes the transport in §3.6 as a notification plus
  `view/promptRespond` request, and fixes the §4.2 row; keep the transport
  unchanged. This is documentation-only.
- **Wire change:** No.
- **Risk:** Low for clients (both follow the code/spec), medium for a
  protocol implementer who reads only ARCHITECTURE/PROTOCOL and builds a
  request handler that never sees an answer.
- **Effort:** S.

### 11. VS Code declares layouts it does not honor

- **Status:** Not real at this baseline; historical, fixed by T004.
- **Evidence:** VS Code declares `layout: ['main']`
  (`clients/vscode/lib/session.js:64-66`), and its text-view path uses
  `vscode.window.showTextDocument` with no `ViewColumn`/placement handling
  (`clients/vscode/lib/views.js:130`). T004 is recorded complete
  (`TASKS_PART_SEVEN.md:215-225`), including the strict typed declaration and
  checked emitter.
- **Proposal:** None. Keep the declaration aligned if a future VS Code client
  gains side/bottom/float placement; that is new work under its own ticket.
- **Wire change:** No.
- **Risk:** None at baseline.
- **Effort:** None.

## Other warts found during the read

### W1. `PROTOCOL` omits `view/invoke.version`

- **Status:** Real (documentation).
- **Evidence:** `PROTOCOL.md:16` lists
  `{viewId, command, args?, cursor?, selection?}`; the actual params include
  `version: Option<u64>` and its positional semantics
  (`src/protocol.rs:277-296`). Clients always send it
  (`clients/vscode/lib/requests.js:64-70`), and rich invoke/key commands use it
  for positional resolution (`src/session/commands.rs:68-77`;
  `src/session.rs:1068-1081,1326-1342`); it does not govern standard LSP reads
  (item 9).
- **Proposal:** Add `version?` to the §4.1 row and cross-reference item 9,
  which also covers the standard-read resolution the table omits.
- **Wire change:** No.
- **Risk:** Low same as item 10.
- **Effort:** S.

### W2. `PROTOCOL` lists keymap binding fields that do not exist

- **Status:** Real (documentation).
- **Evidence:** `PROTOCOL.md:28` documents bindings as
  `{command, keys, modes, label, doc, group, status}`. `Binding` serializes only
  `{command, keys, modes, label, status}` (`src/keymap.rs:43-56`). There is no
  `doc` or `group` field on the wire.
- **Proposal:** Delete `doc` and `group` from the §4.2 row; describe `label`
  and `status`.
- **Wire change:** No.
- **Risk:** Low.
- **Effort:** S.

### W3. A mode-only render reuses the current version

- **Status:** Real (minor ambiguity).
- **Evidence:** Entering text mode builds a `ViewRender` with
  `version: view.version` and neither `text` nor `edits`
  (`src/session/commands.rs:312-330`), while every other delivered change
  increments the version (`src/session.rs:418-461`). Clients update `mode` even
  when there is no text change (`lua/magit-lsp/view.lua:375-394`;
  `clients/vscode/lib/requests.js:125-127`), then acknowledge the same version
  again.
- **Proposal:** Document that a mode-only render is a state notification that
  does not advance the version and carries no document text; clients must apply
  `mode` regardless of render status. Alternatively advance the version, but
  that would make entering text mode look like a document update and is not
  needed by either client.
- **Wire change:** No if documented.
- **Risk:** Low. A client that ignores renders with an unchanged version would
  miss the mode change; neither shipped client does.
- **Effort:** S.

## Fixed, historical, not real, and unknown

- **Fixed / historical:** item 11 (T004 completed the declaration work); the
  "several unacked versions" wording in item 4 (BUG-READ-1 history: no
  multi-version store is retained, though several delivered versions can be
  unacked at once — see item 4); the old TASKS prompt wording in item 10 (a
  historical premise, not proof of a current defect).
- **Not real:** items 3, 6, 11. Item 3 has no remaining cutover mode; item 6
  describes a field that does change behavior; item 11's declaration now
  matches honored layout.
- **Real but documentation-only:** items 1, 2, 4, 9, 10, W1-W3.
- **Real with behavior work:** items 5, 7, 8.
- **Unknown:** none. If a future check cannot establish a claim, label it
  **unknown** rather than inferring a defect; the baseline read did not leave
  one.

## Facts vs assumptions

- **Facts (VERIFIED by code/docs at `f15c026`):** the statuses and line
  references above; the prompt transport is notification plus
  `view/promptRespond`; VS Code is static and receives no `view/keymap`;
  Neovim is dynamic and registers draft keymaps by `runId`; the server emits at
  most one `LineEdit`; `keyModifiers` filters bindings; entering text mode keeps
  the render version unchanged; VS Code declares only `main`.
- **Assumptions (not established by this audit):** whether every client will
  remain static/dynamic as declared; whether future backends reuse the same
  render/edit pipeline unchanged; whether T113/T114 will choose the narrowest
  proposals. The proposals above are options, not decisions; T113 owns the
  choices.
- **Out of scope:** decisions (T113), implementation/cleanup (T114/T105/T119),
  prototypes, and the other session's dirty files. This audit makes no claim
  about the dirty worktree.
