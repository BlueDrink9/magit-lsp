# External client implementation specification

> **Current implementation below:** the custom `view/*` and managed-draft
> wire contract is authoritative today. The Plain-mode section at the end is a
> separate target design; it is not implemented or supported yet. See [Plain
> mode](#plain-mode-standard-lsp-clients-target-specification-not-yet-implemented).

This specification and `src/protocol.rs` are Apache-2.0-licensed; see [LICENSE](LICENSE) and the protocol exception in [LICENSING.md](../LICENSING.md).

This guide describes the current Rust wire contract and bundled clients, plus the separate Plain target design at the end. Rust protocol and dispatch are authoritative for current behavior. A client is a thin transport/editor adapter: server-rendered text is displayed verbatim; structure, Git state, actions and navigation remain server-owned. Core status/menu/stage operation is distinct from optional workflows.

## Capability map (`customRender`)

| Feature | Requirement | Current scope / unsupported behavior |
|---|---|---|
| Standard LSP lifecycle, initialize, status `view/open`, render/apply/ack/resync | Core | Required for any useful session. |
| Structured `view/key` or `view/invoke`, exact displayed cursor/selection/version | Core | Required for navigation and status stage/unstage. Never parse text for structure. |
| Prompt/effect handling | Required when enabling operations that produce them | Reduced clients can explicitly allowlist directly invoked supported commands and avoid unrestricted key/menu forwarding. Hiding UI entries alone is not exclusion. |
| Dynamic `view/keymap` | Optional alternative | `static` or `forwardAll` can replace it, with limitations below. |
| Resize and setMode | Useful | Width-sensitive layout and text mode; not a prerequisite to core status. |
| Managed draft bridge | Optional workflow, required for commit-message editing/history | If absent, exclude draft-producing commands in a reduced allowlist; never edit a Git path as a substitute. |
| Companion/split views | Optional workflow-dependent | Descriptor placement is a hint. A single replaceable window does not automatically preserve simultaneous view lifetimes. |

## Methods and lifecycle

The remainder of this document specifies the current `customRender` contract.

Direction is relative to the language server. Custom capabilities are client preferences, not arbitrary-method negotiation.

| Method | Direction / JSON-RPC kind | Result or purpose |
|---|---|---|
| `initialize` | client → server request (standard) | Standard initialize result/capabilities. Custom client preferences are `params.capabilities.keymapDelivery` (`dynamic` default, `static`, `forwardAll`) and `params.capabilities.keyModifiers` (`full` default, `basic`); profile is in initialization options in both bundled clients. Modifiers are currently parsed/logged, not a negotiated filtering guarantee. |
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
| `textDocument/didOpen`, `didChange`, `didClose` | client → server notifications | Standard full-sync managed draft stream only; didClose detaches, not finish intent. |
| `draft/save`, `draft/finish`, `draft/history` | client → server requests | Versioned checkpoint, finish barrier, history replacement respectively; JSON-RPC request ID correlates result/error. |
| `draft/error` | server → client notification | `{uri,version,method,message,recoverable}`; `version` is nullable (notably for didClose). |
| `window/workDoneProgress/create` | server → client request | `{token}`; answer the matching JSON-RPC request ID (the server currently does not await the response). |
| `$/progress` | server → client notification | `{token,value}`; value kinds are `begin`, `report`, and `end`. |
| `window/workDoneProgress/cancel` | client → server notification | `{token}`; cancel the matching active operation. |
| `window/showMessage` | server → client notification | `{type,message}` standard LSP message. |

The implemented and advertised standard providers are exactly `textDocument/documentSymbol` (document outline), `textDocument/definition` (tree-backed definition/navigation), `textDocument/documentLink` (links; no resolve), `textDocument/foldingRange`, and `textDocument/semanticTokens/full` (full only; no range or delta). Requests identify a document by URI. Positional lookups use the acknowledged tree, falling back to the current tree before the first acknowledgement; they return `null` in text mode. Draft documents use full sync `{openClose:true, change:1}`. The semantic-token legend includes custom `tokenTypeGroups`. No hover, references, formatting, document-link resolve, semantic-token range or delta providers are implemented. Optional providers are not prerequisites for basic status/staging. See `src/server.rs:1371-1431,1883-1926` and `src/session.rs:1581-1590`.

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

Canonical keys use `[modifier+]*key`, lowercase modifiers in `ctrl`, `alt`, `shift`, `meta` order and W3C key names; chord steps are sent individually. `dynamic` consumes complete `view/keymap` replacements atomically. `static` ships generated mappings and requires synchronization with scope/mode/capture. `forwardAll` sends every step and must execute ordinary editor behavior on `passThrough`. Delivery and modifier preferences do not negotiate client feature support or suppress server commands/menu entries; binding status is not a capability gate (`src/protocol.rs:36-60`; `lua/magit-lsp/keymap.lua:108-159`; `clients/vscode/lib/menus.js:37-61`). A reduced client can safely exclude unsupported workflows only by restricting all invocation paths to an explicit, documented allowlist of directly invoked supported commands and not forwarding unrestricted keys or menus. Merely hiding buttons/menu entries is insufficient if other input can still invoke them. A client that forwards unrestricted input must implement dependencies of every reachable operation or state that this reduced mode is not safely supported; there is no automatic disabling. Terminal modifier loss, VS Code layout/IME/dead-key limits, and reserved editor shortcuts constrain capture. Do not intercept native behavior outside the active view/editor focus.

Text mode is disposable local plain-text editing/copying, not a Git edit or managed draft: local changes are not sent to the server tree, synchronized to Git, or preserved when the server replaces the document. Interactive mode is read-only server-rendered display. On entry the server sends a metadata-only render at the unchanged content/version; preserve the text, apply the mode, and acknowledge it (see render rules above). Do not require a native `view/setMode` control: mode changes produced by enabled server keys still arrive in render metadata and must be honored.

In text mode capture is off and only `common.toggle-text-mode` remains bound; other `view/invoke`/`view/key` commands are refused with a readable informational hint, and positional standard-provider requests return `null`. Refresh requests are deferred/queued until exit. On exit, the server applies any queued refresh and sends a full replacement because locally edited text has no shared diff base; discard the local edits and restore read-only interactive display. Never send those edits as managed-draft `didChange` content. Neovim makes the existing buffer modifiable and restores its undo level on entry, then clears undo history and locks it on exit (`lua/magit-lsp/buffer.lua:130-144`). VS Code's virtual content provider remains read-only and queues refreshes (`clients/vscode/lib/session.js:65-72`); do not claim editable text mode there. This is not a mandatory native editable-core capability. Server navigation uses tree metadata; native cursor movement/search may remain available where capture permits. Fold behavior is client-specific and coordinated with server folding, not inferred from text. Styling may use semantic tokens where actually advertised; do not require a particular theme/color.

## Prompts, effects and drafts

### Work-done progress and standard messages

The server emits progress for core key/stage operations. It sends `window/workDoneProgress/create` with a token, then `$/progress` begin (currently title `Git`, operation in `message`, cancellable true), reports as needed, and finally end. Current server quirk: initialize inserts `capabilities.window.workDoneProgress:true` into its result and sends create followed immediately by begin without waiting for the create response or negotiating client support (`src/server.rs:294-343,1903-1914`). A client using an LSP library should let it service standard requests/notifications; a manual transport must correlate and answer create by its JSON-RPC ID and dispatch notifications in both directions. `window/showMessage` is server-to-client `{type,message}` and is used for startup/configuration errors (`src/server.rs:1875-1880,250,824`).

Cancellation is a client-to-server `window/workDoneProgress/cancel` notification carrying the active `{token}`. For example, after receiving begin, send `{"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":"magit-lsp/mutation/1"}}`. The server reports `$/progress` with `value:{"kind":"report","message":"Cancelling…","cancellable":false}` and a separate informational effect, but keeps the operation reservation until its owner reaches terminal end; end alone does not state success. Operation outcomes/errors are delivered separately through effects (`src/server.rs:370-398,1741-1753`). Keep servicing prompt responses and progress-cancel notifications while an operation is outstanding; do not queue them behind that operation's completion. A rich progress UI is optional: correctly servicing this protocol traffic is not.

Prompt kinds are `confirm` (boolean), `text` (string), `choice` (`{id}` or `{custom}`), and `multiChoice` (`{ids:[...]}`). All responses include `cancelled`; cancellation has no value requirement. Honor labels/defaults, choice item IDs and secret masking/no-history behavior where applicable (`src/protocol.rs`, `lua/magit-lsp/prompt.lua`, `clients/vscode/lib/prompt.js`).

Effects serialize with camelCase `type` discriminators, e.g. `openView`, `pushView`, `popView`, `focusView`, `visitFile`, `editDocument`, `passThrough`, `clipboard`, `message`, `quit`—not Rust enum variant names. Messages carry level/text and optional hint/detail/processLogId. Visit-file uses a resolved file URI and zero-based location. Rendering is not an effect. Implement each effect used by enabled commands with readable errors.

Managed drafts are an optional workflow, not a core requirement. The `editDocument` effect payload has all fields `{type:"editDocument",runId,uri,text,version,kind,title,mode,scope}`; current values include `kind:"commit-message"` and `mode:"managedDraft"`. Treat its `magit-commit://` URI as opaque. Open exactly the supplied text and version (currently version 1) with `textDocument/didOpen`; do not synthesize an initial change. Every `didChange` sends exactly one full-text content change, with monotonically increasing positive i32 versions and UTF-8 text no larger than 1 MiB. Save is `draft/save {runId,uri,version}`; history is `draft/history {runId,uri,version,command}`, where command is exactly `commit-message.history-prev` or `commit-message.history-next`; finish is `draft/finish {runId,uri,version,reason}`, with reason `confirm` or `normal-close`. These no-text barriers first require full synchronization, then pin run/URI/version and gate edits until response. Each successful response is an ordinary JSON-RPC result object: `{"type":"draft/checkpointed","runId":...,"uri":...,"version":...}`, `{"type":"draft/finished","runId":...,"uri":...,"version":...}`, or `{"type":"draft/replaced","runId":...,"uri":...,"baseVersion":...,"version":...,"text":...}`; these `type` values are not notification methods. Save checkpoints exactly live text/version without writing or releasing Git. Initial text/version is also the initial saved checkpoint. Confirm must name a version equal to both live and saved; normal-close names the saved version and may discard newer live text. Every accepted history request replaces live text at `baseVersion+1`, even at a history clamp, and leaves saved unchanged. Apply only when baseVersion matches, suppress its didChange echo, then full-sync strictly above returned version. If native text changed despite the editing gate, retain it and reconcile above the returned version rather than dropping it. See `src/protocol.rs:60-159`, `src/server.rs:1169-1213`, `src/editor/draft.rs:138-251,310-316`, `src/editor.rs:165-175`, `lua/magit-lsp/edit.lua:201-219`, `clients/vscode/lib/drafts.js:645-674`.

Correlate each barrier by its JSON-RPC request ID. A rejected full sync can also emit `draft/error {uri,version,method,message,recoverable}`; separately, barrier validation failures are JSON-RPC `InvalidParams` errors whose `data` contains `{runId,uri,version,recoverable}`. Present readable errors, retain the user's local text, invalidate in-flight barriers after a rejected sync, and do not issue finish until synchronization is recovered. `recoverable:true` does not authorize reviving a terminal run. A finish response means editor writeback/helper release, not Git commit success: later effects/progress report the Git operation outcome (`src/server.rs:858-874,919-926,1149-1158,1808-1863`).

Abort explicitly through `view/invoke` on the owning ordinary view with `command:"common.cancel"` and `args:{runId}`; there is no `draft/abort` method, and `textDocument/didClose` only detaches. On normal close, clients choose the saved checkpoint; explicit abort, progress cancellation, disconnect/EOF/shutdown cancel. Neovim classifies intercepted close commands and cancels ambiguous unload/hidden/API-close paths (`lua/magit-lsp/edit.lua:135-188,340-368`). VS Code's explicit close command arbitrates Save/Discard/Cancel, while ambiguous tab/document teardown cancels (`clients/vscode/lib/drafts.js:529-598,819-887`; `clients/vscode/lib/edit.js`). These are observed client differences, not a guarantee that arbitrary native close intent can be inferred: the server bridge is currently Unix-only, with no claim of platform acceptance or tested parity. Drafts are in-memory, not crash durable; late responses cannot resurrect terminal runs. If an unsupported prompt unexpectedly arrives, answer with `cancelled:true` rather than leaving the waiting command suspended. If a managed-draft `editDocument` unexpectedly arrives, do not silently drop it: explicitly abort that owning run with `common.cancel` and `{runId}`.

## Feature-to-capability mapping and clients (`customRender`)

| Workflow | Required client behavior | Present implementation / gap |
|---|---|---|
| Status, menus, stage/unstage | Core view/render/key or invoke, structured context, messages/prompts/effects used | Both clients implement status rendering and command routing (`lua/magit-lsp/view.lua`, `keymap.lua`, `keys.lua`; VS Code `render.js`, `views.js`, `requests.js`, `commands.js`). |
| Commit and managed draft history | Managed editor, full sync, exact save/finish/history barriers, close-intent arbitration | Implemented in Neovim `edit.lua` and VS Code `drafts.js`/`edit.js`; explicit close arbitrates Save/Discard/Cancel, ambiguous tab/document teardown cancels (see draft close behavior above). |
| Log selection / commit picker | Rendered log, selection mode, structured exact-acked commit node; no generic choice picker substitute | Server resolves selection; client forwards input and displays view. |
| Process log | Message detail/process-log handling and view opening if enabled | Optional workflow; do not assume every message includes an ID. |
| File visiting | Handle `visitFile` effect and open resolved URI/location | Optional; not a client-side Git lookup. |
| Visual selections | Send inclusive selection lines with cursor/mode and displayed version | Optional editor-specific interaction; server handles patch selection. |
| Splits/companion views | Multiple descriptor lifetimes and placement effects | Optional; placement fallback is client-specific, never universal single-window assumption. |

Neovim primarily uses dynamic keymaps and buffer-local capture; VS Code uses static contributed bindings plus context/focus guards. Both should remain presentation/input adapters. Consult the listed client sources and `src/session.rs` for actual behavior rather than extrapolating parity from this table.

## Implementation checklist and unresolved decisions (`customRender`)

### Core payload quick reference

`ViewDescriptor` is `{id,kind,uri,title,placement,mode,scope,capture,viewport:{cols,rows}}` (all required); placement serializes `main|side|bottom|float`, view mode `interactive|text`. Treat URI as opaque. `view/invoke` accepts `{viewId,command,version?,args?,cursor?,selection?}`; `view/key` accepts `{viewId,version,key,editorMode,cursor,selection?}` and responds `{effects:[...]}`. Cursor `{line,column}` is zero-based; selection `{startLine,endLine}` is inclusive. `editorMode` is exactly `normal|visual`; key is one canonical step. Every render form requires `mode`; line edits are `{startLine,endLine,lines:string[]}` with half-open line ranges.

`view/keymap` is `{viewId,scope,version,capture,bindings,mode,pending?,liveKeys?}` (actual camelCase `liveKeys`). A binding is `{command,keys,modes,label,status}`; `keys` lists alternative space-separated canonical chords; modes are `normal|visual`; status is `implemented|planned|unavailable`. Dynamic draft keymaps identify `runId` as `viewId`, though drafts are not ordinary `view/open` views; confirm/history use draft barriers, not ordinary view input.

Prompt kinds serialize `confirm|text|choice|multiChoice`; common required fields are `{promptId,kind,title}` and optional fields include `detail,yesLabel,noLabel,destructive,default,placeholder,secret,items,allowCustom,preselected,min,max` as applicable; no prompt `error` field exists. Values are boolean, string, `{id}`/`{custom}`, or `{ids:[...]}` by kind. `view/promptRespond` is `{promptId,cancelled,value?}`. Effects use camelCase `type`: `message` (`level`=`info|warn|error`, `text`, optional `hint/detail/processLogId`), `passThrough{key}`, `clipboard{text}`, `openView{descriptor}`, `pushView{descriptor}`, `popView{viewId}`, `focusView{viewId}`, `visitFile{uri,line,column}` (all required; zero-based location), `editDocument`, `quit`. Progress is separate LSP traffic, never an effect.

Presentation differs: Neovim displays rendered menu buffers; VS Code may present structured menu data via QuickPick. VS Code pass-through replay and placement are limited/concrete client behavior, not universal parity. Buffer render/keymap notifications until the descriptor/effect is registered. Progress notifications may be emitted before the request response; deferred view notifications follow the response and can arrive before client-side view registration. Dynamic keymaps are complete replacements; do not infer new render versions from them.

The following is an abridged illustrative exchange (IDs/text are illustrative). The initialize response and keymap are abridged; the keymap bindings are valid JSON examples only and are not an installable complete map:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":1234,"rootUri":"file:///work/project","capabilities":{"window":{"workDoneProgress":true},"textDocument":{"synchronization":{"dynamicRegistration":false,"didSave":true}},"keymapDelivery":"dynamic","keyModifiers":"full"},"initializationOptions":{"profile":"standard"}}}
{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"textDocumentSync":{"openClose":true,"change":1},"window":{"workDoneProgress":true}}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"view/open","params":{"kind":"status","args":{"repo_path":"/work/project"},"placement":"main","viewport":{"cols":80,"rows":24}}}
{"jsonrpc":"2.0","id":2,"result":{"id":"view-1","kind":"status","uri":"magit://status/project","title":"Status","placement":"main","mode":"interactive","scope":"status","capture":false,"viewport":{"cols":80,"rows":24}}}
{"jsonrpc":"2.0","method":"view/render","params":{"viewId":"view-1","version":1,"text":["On branch main",""],"cursor":{"line":0,"column":0},"mode":"interactive"}}
{"jsonrpc":"2.0","method":"view/keymap","params":{"viewId":"view-1","scope":"status","version":1,"capture":false,"mode":"interactive","bindings":[{"command":"common.refresh","keys":["g"],"modes":["normal"],"label":"Refresh","status":"implemented"}]}}
{"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":"view-1","version":1}}
{"jsonrpc":"2.0","id":3,"method":"view/key","params":{"viewId":"view-1","version":1,"key":"g","editorMode":"normal","cursor":{"line":0,"column":0}}}
{"jsonrpc":"2.0","id":4,"method":"window/workDoneProgress/create","params":{"token":"magit-lsp/example/1"}}
{"jsonrpc":"2.0","method":"$/progress","params":{"token":"magit-lsp/example/1","value":{"kind":"begin","title":"Git","message":"Refreshing","cancellable":true}}}
{"jsonrpc":"2.0","id":4,"result":null}
{"jsonrpc":"2.0","method":"$/progress","params":{"token":"magit-lsp/example/1","value":{"kind":"end"}}}
{"jsonrpc":"2.0","id":3,"result":{"effects":[]}}
{"jsonrpc":"2.0","method":"view/render","params":{"viewId":"view-1","version":2,"baseVersion":1,"edits":[],"mode":"interactive"}}
{"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":"view-1","version":2}}
```

Progress creation and notifications are separate from the key result effects and occur for ordinary key requests. For this synchronous refresh, progress end precedes the key response; the server does not wait for the create response, so it may arrive after begin. The illustrative unchanged refresh advances the render version with empty edits. Payload definitions are grounded in `src/view.rs`, `src/keymap.rs`, `src/protocol.rs`; presentation details in `lua/magit-lsp/view.lua`, `lua/magit-lsp/edit.lua`, `clients/vscode/lib/views.js`, `menus.js`, and `effects.js`.

- [ ] Implement standard lifecycle and only actual advertised standard providers.
- [ ] Open status, retain descriptor/opaque URI, verbatim apply, ack after apply, resync on loss.
- [ ] Choose dynamic/static/forwardAll and document capture limitations; preserve native pass-through.
- [ ] Forward structured cursor, inclusive selection, editor mode and displayed version.
- [ ] Implement prompt kinds and effects required by the enabled command set.
- [ ] Add resize/text mode and multiple placements only where needed.
- [ ] Implement managed drafts only with ordered full sync, barriers, history replacement and explicit close-intent semantics.

Unresolved/follow-up: define support expectations for external clients' optional effects; decide whether any future method-level capability discovery is needed (none currently); document interoperability expectations for clients that cannot distinguish intentional close from teardown; decide whether multi-view placement needs stronger client lifecycle guidance. These are not current negotiation mechanisms.

**Source cross-check (`customRender`):** `src/protocol.rs` (wire types/serialization), `src/server.rs` (dispatch/lifecycle), `src/view.rs` (descriptor), `src/session.rs` (render/ack retention); Neovim `lua/magit-lsp/init.lua`, `view.lua`, `buffer.lua`, `requests.lua`, `keymap.lua`, `keys.lua`, `prompt.lua`, `effects.lua`, `edit.lua`; VS Code `clients/vscode/lib/session.js`, `requests.js`, `render.js`, `views.js`, `commands.js`, `prompt.js`, `effects.js`, `drafts.js`, `edit.js`. Re-check these sources as contracts evolve.

## Plain mode: standard-LSP clients (target specification; not yet implemented)

This section specifies the adopted target standard-LSP presentation, pending
the implementation spike and subsequent acceptance; it does not change the
current `customRender` implementation. Do not infer successful implementation,
harness runs, native traces, support-table generation, or Helix spike results.

### Selection and bootstrap

Clients select presentation with `initializationOptions.presentation`:
`"customRender"` selects the bundled/current adapter; absent selects target
Plain mode. Bundled clients explicitly select `customRender`. Plain clients
bootstrap by opening a `.magit` document associated with a workspace/document
repository context. The backend resolves and canonicalizes repository identity,
secures paths, and excludes presentation documents from untracked status. If
multiple repositories are plausible, it serves a choose-repository view; it
must never guess for a mutation.

For active core views, `workspace.applyEdit` is required. Without it the view is
readable but inert. `workspace.workspaceEdit.documentChanges` is recommended;
fall back to unversioned `WorkspaceEdit.changes` when absent. `window.showDocument.support`
is required for workflows that edit a provider file, but optional for navigation
(which can degrade to a path/location message). `window.workDoneProgress` is
optional: omit progress when unsupported, and await the `window/workDoneProgress/create`
response before sending begin. `general.positionEncodings` is optional; default
to UTF-16 when absent. `window/showMessage` is standard baseline behavior.
Recommended providers are document symbols, definition, links, folding and
semantic tokens; completion is a server provider for fields, not a client
capability. Server sync options (`textDocumentSync`) are advertised by the server,
not client capabilities. This is target policy, not current server negotiation.

### Render synchronization and actions

The target server advertises incremental sync and accepts ranged and rangeless
`didChange` sequences, applying each change in order in a URI-keyed document
store. Use the negotiated UTF-8, UTF-16, or UTF-32 position encoding for every
range; versions are opaque document versions and need not increment by one.
Prefer `WorkspaceEdit.documentChanges` with document versions. Neither the
client nor server assumes a render is version V+1.

Each document has a one-flight apply pipeline: `Ready` → `Applying` → `Ready`,
or `Detached` on close/disconnect. While applying, coalesce newer authoritative
renders. Track bounded submitted text/version/hash metadata. A matching
`didChange` echo is recognized by text hash whether it arrives before or after
the apply response; it is not mistaken for user input. On stale apply, obtain a
fresh snapshot and retry; on divergence, repair from authoritative text. Never
advance the applied state merely because an edit was submitted.

Standard providers and code actions use a matched document/tree snapshot.
Completion candidates come from typed, provider-supplied sources; each item
supplies its label, insertion text, and optional detail/kind, with no client Git
lookup or parsing. The same provider data may populate document symbols,
definitions, links, folding and semantic tokens where available. Code-action
discovery is side-effect-free and runs no Git subprocess. Resolve against the
snapshot whose text hash matches the synced document; if none matches, return no
actions and null positional providers. Actions have stable command IDs with
configurable titles. `Planned` and
`Unavailable` actions are disabled with a reason. Convert LSP half-open ranges
to inclusive selections explicitly; an end at column zero belongs to the
previous line. `actions.title_style` has exactly four values: `plain`,
`mnemonic`, `path`, and `mnemonicPath`.

Actions use `workspace/executeCommand` command `magit.command`. The proposed
argument is exactly one opaque object:
`arguments=[{command,documentUri,target:{node_id,mutation_hash},viewGeneration}]`.
IDs and arguments are opaque to clients. The backend revalidates node
identity and mutation hash against fresh state at execution; stale targets cause
refresh and no mutation. One semantic dispatcher serves these actions and the
existing custom input adapter.

### Views, prompts and reusable server interactions

The client displays one server-owned document verbatim. Logical concrete views,
menus and selection views can replace/stack within it while the server retains
view identity, menu state and caller restoration. Stable-ID single/multi-select
operations return to their caller on confirm or cancel. These reusable server
interactions are parameterized by concrete views/data: they do not imply a
client picker, invented wire methods, or a broad VCS trait hierarchy.

Prompts are document-backed fields for confirm, text, choice and multiChoice.
Fields are bounded, tracked, single-line `Field` tree nodes; only user-authored
field values are read from the document store. Edits outside fields are repaired
to authoritative text. Completion is supplied by typed providers. Menus and
prompts offer actions on every line and every non-root view has Back. Views are
expanded by default; sections over 400 lines are collapsed with an expand action
(the limit is configurable). The single
effect/prompt capability mapping is in the [Plain task list](../magit-lsp-plain-client-tasks.md#2-plain-mode-effect-and-prompt-support-the-one-capability-table);
it is the planned mapping and is not duplicated here. Secrets and askpass are
unsupported: cancel cleanly with a message; credential-dependent network work
fails readably.

### Commit editing: current managed draft and proposed real-file workflow

The current implemented contract remains the managed virtual draft described
above: opaque `magit-commit://` URI, full sync, and `draft/save`, `draft/finish`,
and `draft/history` version barriers. It has explicit close-intent arbitration;
`didClose` only detaches. Do not describe the real-file workflow below as
implemented or remove the current contract before gated cutover.

**Proposed R-EDIT-REAL target (not implemented):** Git supplies the normal,
Git-generated `COMMIT_EDITMSG` template and retains ownership of commit,
comment stripping, empty-message behavior, hooks and helper lifecycle. The
backend owns canonical path discovery/security and refresh; the client treats
the supplied file URI as opaque. Native save alone does not finish. A native
`didClose` or positively identified last-tab close confirms the saved file text;
explicit confirm instead validates the synchronized buffer version and its
pending changes before finish. On dirty close, use the native Save/Discard/Cancel
choice: Save confirms the saved file, Discard confirms the previously saved
text (not the dirty buffer), and Cancel leaves the session active. Explicit
abort cancels and is distinct from confirm. Preserve Git's normal comment and
empty-message semantics; do not substitute a custom template or `git commit -F`.

For VS Code, investigate an optional `tabGroups.onDidChangeTabs` observer and
confirm only when the last managed tab for that session closes. Focus changes,
editor switches and non-last tab closure are not confirmation; do not synthesize
`didClose`. Keep managed tabs permanent (`preview:false`; account for
`showDocument` middleware). The existing `onDidChangeTabs` teardown handling and
`preview:false` behavior are implementation conflicts to resolve, not proof the
real-file fallback works. Retain explicit confirm/abort for clients without a
reliable close signal. `magit.confirmEdit` is presently a local VS Code command;
the target must establish its availability/registration in the applicable
workflow. `workspace/executeCommand` is a proposed Plain action mechanism, not
an already-registered server command; do not invent a custom protocol method.

`didClose`, explicit confirmation, cancellation, shutdown and tab closure must
remain distinct, session-aware, idempotent outcomes. Shutdown/reload/workspace
change is not confirmation; whether shutdown can emit `event.closed` remains an
assumption to verify. The existing native evidence records VS Code `didClose`
failure; the proposed extension tab observer is a new fallback that must be
verified independently, including that shutdown cannot be mistaken for user
confirmation. See R-EDIT-REAL for the spike and implementation gates.

### Generic Helix reference and conformance

Illustrative generic Helix TOML (no editor-specific expansions):

```toml
[language-server.magit-lsp]
command = "magit-lsp"

[[language]]
name = "magit"
scope = "source.magit"
file-types = [{ glob = "*.magit" }]
roots = [".git"]
language-servers = ["magit-lsp"]
```

This is a reference configuration, not a support claim or verified smoke test.
The editor helper is required; repository resolution has no launcher.
Commit-file LSP attachment and reuse of an existing connection remain unverified
smoke requirements; this configuration does not claim they work.
Conformance checklist for the seven harness profiles: `decomposed-changes`,
`debounced-didchange`, `reorder-echo`, `no-documentChanges`, `no-showDocument`,
`crlf-normalise`, and `utf16`. All harness runs, native evidence, and Helix
manual smoke remain pending. The source/runtime cross-check also remains pending.
