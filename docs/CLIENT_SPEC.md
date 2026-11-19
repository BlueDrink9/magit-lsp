# External client implementation specification

> **Current implementation below:** the custom `view/*` and real-file commit
> wire contract is the Level 4 rich protocol, used by the bundled clients. The
> Plain presentation at the end (Levels 0–3) is implemented and tested. See
> [Client implementation levels](#client-implementation-levels).

This specification and `src/protocol.rs` are Apache-2.0-licensed; see [LICENSE](LICENSE) and the protocol exception in [LICENSING.md](../LICENSING.md).

This guide describes the current Rust wire contract, its Plain presentation levels (0–3) and the rich Level 4 protocol used by the bundled clients. Rust protocol and dispatch are authoritative for current behavior. A client is a thin transport/editor adapter: server-rendered text is displayed verbatim; structure, Git state, actions and navigation remain server-owned. Core status/menu/stage operation is distinct from optional workflows.

## Capability map (`customRender`)

| Feature | Requirement | Current scope / unsupported behavior |
|---|---|---|
| Standard LSP lifecycle, initialize, status `view/open`, render/apply/ack/resync | Core | Required for any useful session. |
| Structured `view/key` or `view/invoke`, exact displayed cursor/selection/version | Core | Required for navigation and status stage/unstage. Never parse text for structure. |
| Prompt/effect handling | Required when enabling operations that produce them | Reduced clients can explicitly allowlist directly invoked supported commands and avoid unrestricted key/menu forwarding. Hiding UI entries alone is not exclusion. |
| Dynamic `view/keymap` | Optional alternative | `static` can replace it, with limitations below. |
| Resize and setMode | Useful | Width-sensitive layout and text mode; not a prerequisite to core status. |
| Commit editor bridge | Optional workflow, required for commit-message editing | Opens the exact announced real COMMIT_EDITMSG URI; exclude producing commands if unsupported. History recall is planned. |
| Companion/split views | Optional workflow-dependent | Descriptor placement is a hint. A single replaceable window does not automatically preserve simultaneous view lifetimes. |

## Methods and lifecycle

The remainder of this document specifies the current `customRender` contract.

Direction is relative to the language server. Custom capabilities are client preferences, not arbitrary-method negotiation.

In production, undeclared optional effects become warning-message effects, and
undeclared non-main view placements fall back to `main` without changing the
view descriptor identity. Undeclared prompts become warning effects.
`cfg(test)` library builds assert at emission time before these fallbacks;
`allows_prompt` remains a non-panicking preflight check. No declaration keeps
the legacy unrestricted behavior.

| Method | Direction / JSON-RPC kind | Result or purpose |
|---|---|---|
| `initialize` | client → server request (standard) | `capabilities.experimental.magit` declares rich v1; absence selects Plain. The declaration carries `v` (selected session version; only `1`) and optional `minimumVersion` (oldest usable session version; defaults to `v`, must be a positive integer not above `v`). The result advertises `capabilities.experimental.magit.protocolVersion` (currently `1`). Key preferences are `keys` and `modifiers` inside that block. Profile remains in initialization options. |
| `initialized` | client → server notification | Standard lifecycle. |
| `shutdown` / `exit` | client → server request / notification | Standard lifecycle; pending drafts/processes are cancelled on connection shutdown/disconnect. |
| `workspace/didChangeConfiguration` | client → server notification | Triggers configuration reload; do not assume its settings payload is ingested. |
| `view/open` | client → server request | `{kind,args?,placement?,viewport}` → `ViewDescriptor`; status repository is in `args.repo_path`. |
| `view/close` | client → server request | `{viewId}`; closes a view. |
| `view/invoke` | client → server request | Command and optional args/cursor/selection/version → `{effects}`. |
| `view/key` | client → server request | One canonical key step plus editor mode, cursor, selection?, version → `{effects}`. |
| `view/render` | server → client notification | Full `{version,text}`, incremental `{version,baseVersion,edits}`, or metadata-only `{version,mode,pending?,menu?}` render. Metadata-only preserves content and version while applying metadata and acknowledging application. |
| `view/rendered` | client → server notification | Ack exact applied `{viewId,version}`. |
| `view/resync` | client → server request | `{viewId}` → current full render; replace local content. |
| `view/resize` | client → server notification | `{viewId,cols,rows}`. |
| `view/setMode` | client → server request | `{viewId,mode}` → effects. |
| `view/keymap` | server → client notification | Complete replacement bindings for the scope, with capture/mode/pending/live keys. Normal notices use the view render version; draft keymaps use 0. This is separate bookkeeping, not a render ack or guaranteed newly incremented counter. Static delivery does not require pushes. |
| `view/prompt` | server → client notification | Prompt request data; answer through `view/promptRespond`. |
| `view/promptRespond` | client → server request | `{promptId,cancelled,value?}`; result is currently generic JSON value. |
| `view/effects` | server → client notification | Asynchronous effects, e.g. draft editor opened after an action returned. |
| `textDocument/didOpen`, `didChange`, `didClose` | client → server notifications | Full-sync commit file stream; ordinary didClose confirms saved disk bytes (VSCode exception below). |
| `workspace/executeCommand` | client → server request | `magit.command` with one run-aware semantic common.confirm argument; see real-file contract below. |
| `window/workDoneProgress/create` | server → client request | `{token}`; answer the matching JSON-RPC request ID (the server currently does not await the response). |
| `$/progress` | server → client notification | `{token,value}`; value kinds are `begin`, `report`, and `end`. |
| `window/workDoneProgress/cancel` | client → server notification | `{token}`; cancel the matching active operation. |
| `window/showMessage` | server → client notification | `{type,message}` standard LSP message. |

**Version handshake (server and Neovim implemented; VS Code pending).** The
server strictly accepts `v = 1`, accepts an optional `minimumVersion` when it
is a positive integer not above `v`, and rejects any other declaration with a
readable initialize error before creating a session or opening a rich view. It
advertises `capabilities.experimental.magit.protocolVersion = 1` as the version
of every accepted rich session. Neovim declares `v = 1` and `minimumVersion = 1`
and requires a matching positive-integer advertisement before opening views or
sending rich traffic. Missing, malformed or different advertisements produce a
readable required/advertised-version error and stop the transport. An initialize
rejection reports the server's explanation and stops the transport, without
retry or downgrade. The package-version warning remains separate. VS Code's
declaration and advertisement check are pending the remaining T114 parts.
Clients must treat an initialize rejection as a hard failure; there is no
fallback to Plain.

The implemented and advertised standard providers are exactly `textDocument/documentSymbol` (document outline), `textDocument/definition` (tree-backed definition/navigation), `textDocument/documentLink` (links; no resolve), `textDocument/foldingRange`, and `textDocument/semanticTokens/full` (full only; no range or delta). Rich and Plain share these providers, plus advertised code actions, `magit.command` execute command, and document synchronization. Plain code actions/commands are implemented semantic-action dispatch, not future targets. Requests identify a document by URI. Positional lookups use the acknowledged tree, falling back to the current tree before the first acknowledgement; they return `null` in text mode. Draft documents use full sync `{openClose:true, change:1}`. The semantic-token legend includes custom `tokenTypeGroups`. No completion, hover, references, rename, diagnostics, formatting, document-link resolve, semantic-token range or delta providers are implemented. Plain completion candidates and broader Git/workflow providers remain target work. Optional providers are not prerequisites for basic status/staging. See `src/server.rs:4100-4150`.

## Descriptors, URIs and view lifetime

`ViewDescriptor` carries server-issued identity, kind, URI, title, placement, mode, scope and capture state (`src/view.rs`). Treat the URI as opaque; retain the descriptor and associate it with the server view ID. Placement is a hint, not a command to create a particular native layout. `view/close` closes one view; push/pop effects describe stack behavior but clients still decide concrete buffers/panels/windows. Do not assume replacing one window handles every lifetime or companion workflow. A single-window client may replace the visible presentation, but must retain prior view identities and restore/focus them on pop; optional splits do not remove push/pop/focus lifetime bookkeeping (`src/server.rs:1667-1716,1777-1800`).

Render/application ordering:

   1. On open, retain the descriptor and await/apply the initial render. Render and keymap notifications have separate bookkeeping; ordinary keymap notices use the view render version, while draft keymaps use 0. A keymap notification is a complete replacement, not a render acknowledgement or a guaranteed newly incremented counter.
2. Apply a full render by replacing all text. No `baseVersion` is required for a full render.
   3. If `text` is present, it takes precedence: replace all text regardless of edits. Otherwise apply edits only if local content is exactly `baseVersion`; line ranges are half-open `[startLine,endLine)`. The current producer emits zero or one prefix/suffix replacement. Both bundled clients apply supplied edits bottom-up by descending `startLine`; arbitrary multiple/overlapping edit ordering semantics are not defined.
4. Render text lines verbatim. A metadata-only render has neither `text` nor `edits`: preserve text and version, apply its mode/pending/menu/cursor metadata, then acknowledge application. Apply metadata accompanying content renders as well. Only after application send `view/rendered` for that version.
5. Server retains only current and last acknowledged trees. Positional commands must name the displayed version; stale or omitted positional versions are refused. Commit selection confirm/toggle-mark requires exact acknowledged version. Non-positional commands ignore version.
6. If a base is unavailable, update is lost, or local state cannot be proven to match, request `view/resync`; replace with returned full render, then ack. Never ack before application.

Selections use inclusive document line endpoints (`startLine` through `endLine`); line edits use exclusive ends. Cursor is zero-based. Pass structured cursor/selection and editor mode, never derive them by parsing display lines (`src/protocol.rs`, `src/tree.rs`, `src/session.rs`).

## Input, navigation and presentation

Canonical keys use `[modifier+]*key`, lowercase modifiers in `ctrl`, `alt`, `shift`, `meta` order and W3C key names; chord steps are sent individually. `dynamic` consumes complete `view/keymap` replacements atomically. `static` ships generated mappings and requires synchronization with scope/mode/capture. Delivery and modifier preferences do not negotiate client feature support or suppress server commands/menu entries; binding status is not a capability gate (`src/protocol.rs:36-60`; `lua/magit-lsp/keymap.lua:108-159`; `clients/vscode/lib/menus.js:37-61`). A reduced client can safely exclude unsupported workflows only by restricting all invocation paths to an explicit, documented allowlist of directly invoked supported commands and not forwarding unrestricted keys or menus. Merely hiding buttons/menu entries is insufficient if other input can still invoke them. A client that forwards unrestricted input must implement dependencies of every reachable operation or state that this reduced mode is not safely supported; there is no automatic disabling. Terminal modifier loss, VS Code layout/IME/dead-key limits, and reserved editor shortcuts constrain capture. Do not intercept native behavior outside the active view/editor focus.

Text mode is disposable local plain-text editing/copying, not a Git edit or managed draft: local changes are not sent to the server tree, synchronized to Git, or preserved when the server replaces the document. Interactive mode is read-only server-rendered display. On entry the server sends a metadata-only render at the unchanged content/version; preserve the text, apply the mode, and acknowledge it (see render rules above). Do not require a native `view/setMode` control: mode changes produced by enabled server keys still arrive in render metadata and must be honored.

In text mode capture is off and only `common.toggle-text-mode` remains bound; other `view/invoke`/`view/key` commands are refused with a readable informational hint, and positional standard-provider requests return `null`. Refresh requests are deferred/queued until exit. On exit, the server applies any queued refresh and sends a full replacement because locally edited text has no shared diff base; discard the local edits and restore read-only interactive display. Never send those edits as managed-draft `didChange` content. Neovim makes the existing buffer modifiable and restores its undo level on entry, then clears undo history and locks it on exit (`lua/magit-lsp/buffer.lua:130-144`). VS Code's virtual content provider remains read-only and queues refreshes (`clients/vscode/lib/session.js:65-72`); do not claim editable text mode there. This is not a mandatory native editable-core capability. Server navigation uses tree metadata; native cursor movement/search may remain available where capture permits. Fold behavior is client-specific and coordinated with server folding, not inferred from text. Styling may use semantic tokens where actually advertised; do not require a particular theme/color.

## Prompts, effects and drafts

### Work-done progress and standard messages

The server emits progress for core key/stage operations. It sends `window/workDoneProgress/create` with a token, then `$/progress` begin (currently title `Git`, operation in `message`, cancellable true), reports as needed, and finally end. Current server quirk: initialize inserts `capabilities.window.workDoneProgress:true` into its result and sends create followed immediately by begin without waiting for the create response or negotiating client support (`src/server.rs:294-343,1903-1914`). A client using an LSP library should let it service standard requests/notifications; a manual transport must correlate and answer create by its JSON-RPC ID and dispatch notifications in both directions. `window/showMessage` is server-to-client `{type,message}` and is used for startup/configuration errors (`src/server.rs:1875-1880,250,824`).

Cancellation is a client-to-server `window/workDoneProgress/cancel` notification carrying the active `{token}`. For example, after receiving begin, send `{"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":"magit-lsp/mutation/1"}}`. The server reports `$/progress` with `value:{"kind":"report","message":"Cancelling…","cancellable":false}` and a separate informational effect, but keeps the operation reservation until its owner reaches terminal end; end alone does not state success. Operation outcomes/errors are delivered separately through effects (`src/server.rs:370-398,1741-1753`). Keep servicing prompt responses and progress-cancel notifications while an operation is outstanding; do not queue them behind that operation's completion. A rich progress UI is optional: correctly servicing this protocol traffic is not.

Prompt kinds are `confirm` (boolean), `text` (string), `choice` (`{id}` or `{custom}`), and `multiChoice` (`{ids:[...]}`). All responses include `cancelled`; cancellation has no value requirement. Honor labels/defaults, choice item IDs and secret masking/no-history behavior where applicable (`src/protocol.rs`, `lua/magit-lsp/prompt.lua`, `clients/vscode/lib/prompt.js`).

Effects serialize with camelCase `type` discriminators, e.g. `pushView`, `popView`, `focusView`, `visitFile`, `editDocument`, `passThrough`, `clipboard`, `message`, `quit`—not Rust enum variant names. Messages carry level/text and optional hint/detail. Visit-file uses a resolved file URI and zero-based location. Rendering is not an effect. Implement each effect used by enabled commands with readable errors.

### Current real-file commit editing

The effect is `{type:"editDocument",runId,uri,text,version:1,
kind:"commit-message",title,mode:"realFile",scope:"commit-message"}`. Open the
exact canonical `file:` URI in a permanent native editor, not a virtual provider.
Use one full-sync stream (bounded UTF-8, 1 MiB, strictly increasing i32 versions).
Version 1 is for new runs; recovery reuses the pending run's accepted snapshot and
version. Send one full-text change per didChange, never a second custom text channel.
Native save writes the file and stays editing. Explicit confirmation sends
`workspace/executeCommand {command:"magit.command",arguments:[{
command:"common.confirm",runId,uri,version,source:"synchronized"}]}` after flushing
the exact latest text. The server validates that version and writes it safely.
For saved-file close use `source:"saved-file"` instead: the server reads saved disk
bytes and never substitutes unsaved didChange text. Successful response is
`{type:"confirmed",runId,uri,version}`; Git success is reported separately.
Only these draft confirmation and Abort semantic commands are registered here,
not the full Plain dispatcher.

Ordinary `didClose` confirms saved disk bytes by accepted assumption, despite
standard LSP's lack of run identity/reliability guarantees. VSCode identifies
itself in clientInfo; its didClose is detach-only. Its adapter confirms only an
actual closed event for an owned last matching URI tab across all groups, using
the run-aware semantic saved-file command. Focus changes, nonlast tab closure,
deactivation/disconnect and language changes do not confirm. No synthetic didClose
or capability negotiation is used. Native dirty-close Save saves then confirms;
Discard-unsaved confirms prior saved bytes; Cancel retains the editor. Explicit
Abort sends `workspace/executeCommand {command:"magit.command",arguments:[{
command:"common.cancel",runId,uri,version}]}` for the exact synchronized draft
version. Success returns `{type:"aborted",runId,uri,version}`. On rejection or
missing or mismatched acknowledgement, retain the editable draft; only the exact
acknowledgement ends it.

Helper target/path/owner/run validation, exact-version errors and read/write
failures retain the helper. Registry locking arbitrates terminal/abort decisions
once. Native safe-save replacement in the pinned directory is permitted, but
symlinks, hardlinks, foreign owners and unrelated paths are rejected. Git owns
effective-empty/template/cleanup semantics. Rejected sync produces a readable
message effect, retaining local edits. A rejected ordinary close re-presents the
same pending run and accepted text/version; retain local text, restore an editable
buffer and its sync/keymaps, and allow repair/retry without releasing the helper.
The bounded in-memory per-repository ring (50 entries / 1 MiB aggregate UTF-8,
exact consecutive dedup and oldest eviction) retains eligible saved submissions and
best-effort saved text on abort, never didSave alone. Ring parity remains unverified;
rich history recall is removed/planned under R-EDIT-HISTORY. No `draft/*` wire
methods, draft-error notification or managed provider remain. Recovery is best
effort, not crash durability. The bridge is Unix-only; platform acceptance is not
implied. Focused production native evidence exists in
[`ARCHITECTURE.md`](ARCHITECTURE.md#310-editor-bridge-open-gits-editor-inside-the-users-editor), but parent
R-EDIT-REAL broad native/adversarial/full-phase acceptance remains pending.

The former virtual managed-draft protocol and custom dirty-close picker are
superseded; their historical evidence is not acceptance of this workflow.
If an unsupported prompt arrives, answer with `cancelled:true`. If an unsupported
`editDocument` arrives, explicitly abort its owning run with `common.cancel` and
`{runId}` rather than leaving Git suspended. Correlate semantic responses by their
JSON-RPC request IDs; errors must remain readable and must not resurrect terminal runs.

## Feature-to-capability mapping and clients (`customRender`)

The generated capability declaration summary is [what you get for what you declare](schema/capability-presets.md). It is generated from the five native Plain presets by `make update-capability-presets`.

| Workflow | Required client behavior | Present implementation / gap |
|---|---|---|
| Status, menus, stage/unstage | Core view/render/key or invoke, structured context, messages/prompts/effects used | Both clients implement status rendering and command routing (`lua/magit-lsp/view.lua`, `keymap.lua`, `keys.lua`; VS Code `render.js`, `views.js`, `requests.js`, `commands.js`). |
| Commit editing | Real Git file, full sync, exact synchronized confirmation or saved-file close, explicit abort and recovery | Implemented in Neovim `edit.lua` and VS Code `drafts.js`/`edit.js`; native dirty-close behavior above. History recall is planned, not implemented. |
| Log selection / commit picker | Rendered log, selection mode, structured exact-acked commit node; no generic choice picker substitute | Server resolves selection; client forwards input and displays view. |
| Process log | Separate process-log commands and views | Optional workflow; do not link process-log views to message payloads. |
| File visiting | Handle `visitFile` effect and open resolved URI/location | Optional; not a client-side Git lookup. |
| Visual selections | Send inclusive selection lines with cursor/mode and displayed version | Optional editor-specific interaction; server handles patch selection. |
| Splits/companion views | Multiple descriptor lifetimes and placement effects | Optional; placement fallback is client-specific, never universal single-window assumption. |

| Client | Source-checked behavior and limits |
|---|---|
| Neovim | Dynamic keymaps and buffer-local capture (`lua/magit-lsp/init.lua`, `view.lua`). Modifier declaration is GUI/terminal-specific; do not promise `full` unconditionally. |
| VS Code | Static contributed bindings plus focus guards (`clients/vscode/lib/session.js`). Presents main/bottom layouts and menus with QuickPick; pass-through replay and placement are limited (`views.js`, `menus.js`, `effects.js`). |
| Plain | Standard document and code-action client; no clipboard or secret-prompt support. Optional capabilities degrade as documented in [Client implementation levels](#client-implementation-levels). `tests/ui/plain_init.lua` is the minimal reference client. |

These are client-specific, not parity guarantees. Consult the listed sources and `src/session.rs` for actual behavior.

## Implementation checklist and unresolved decisions (`customRender`)

### Core payload quick reference

Payload schemas are maintained in [Client payloads](client-payloads.md#core-payload-quick-reference).

Worked exchanges and implementation details are maintained in [Client exchanges](client-exchanges.md#worked-exchanges).

- [ ] Implement standard lifecycle and only actual advertised standard providers.
- [ ] Open status, retain descriptor/opaque URI, verbatim apply, ack after apply, resync on loss.
- [ ] Choose dynamic or static key delivery and document capture limitations; preserve native pass-through behavior where applicable.
- [ ] Forward structured cursor, inclusive selection, editor mode and displayed version.
- [ ] Implement prompt kinds and effects required by the enabled command set.
- [ ] Add resize/text mode and multiple placements only where needed.
- [ ] Implement real-file editing with ordered full sync, distinct confirmation sources, native close/abort and rejected-close recovery.

Unresolved/follow-up: define support expectations for external clients' optional effects; decide whether any future method-level capability discovery is needed (none currently); document interoperability expectations for clients that cannot distinguish intentional close from teardown; decide whether multi-view placement needs stronger client lifecycle guidance. These are not current negotiation mechanisms.

**Source cross-check (`customRender`):** `src/protocol.rs` (wire types/serialization), `src/server.rs` (dispatch/lifecycle), `src/view.rs` (descriptor), `src/session.rs` (render/ack retention); Neovim `lua/magit-lsp/init.lua`, `view.lua`, `buffer.lua`, `requests.lua`, `keymap.lua`, `keys.lua`, `prompt.lua`, `effects.lua`, `edit.lua`; VS Code `clients/vscode/lib/session.js`, `requests.js`, `render.js`, `views.js`, `commands.js`, `prompt.js`, `effects.js`, `drafts.js`, `edit.js`. Re-check these sources as contracts evolve.

[Generated known-client behaviors](known-client-behaviors.md)

## Client implementation levels

See the [client implementation levels reference](client-implementation-levels.md#client-implementation-levels).

### Generic Helix reference configuration

See the [Helix reference configuration](client-helix-reference.md#generic-helix-reference-configuration).
