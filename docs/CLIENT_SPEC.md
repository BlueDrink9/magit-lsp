# External client implementation specification

This guide describes the current Rust wire contract and bundled clients, not a future promise. Rust protocol and dispatch are authoritative. A client is a thin transport/editor adapter: server-rendered text is displayed verbatim; structure, Git state, actions and navigation remain server-owned. Core status/menu/stage operation is distinct from optional workflows.

## Capability map

| Feature | Requirement | Current scope / unsupported behavior |
|---|---|---|
| Standard LSP lifecycle, initialize, status `view/open`, render/apply/ack/resync | Core | Required for any useful session. |
| Structured `view/key` or `view/invoke`, exact displayed cursor/selection/version | Core | Required for navigation and status stage/unstage. Never parse text for structure. |
| Prompt/effect handling | Required when enabling operations that produce them | Do not expose dependent operations if an effect or prompt cannot be handled. No command-disable negotiation exists. |
| Dynamic `view/keymap` | Optional alternative | `static` or `forwardAll` can replace it, with limitations below. |
| Resize and setMode | Useful | Width-sensitive layout and text mode; not a prerequisite to core status. |
| Managed draft bridge | Optional workflow, required for commit-message editing/history | If absent, do not enable that workflow; never edit a Git path as a substitute. |
| Companion/split views | Optional workflow-dependent | Descriptor placement is a hint. A single replaceable window does not automatically preserve simultaneous view lifetimes. |

## Methods and lifecycle

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
| `window/workDoneProgress/create` | server → client request | `{token}`; answer the matching JSON-RPC request ID (the server currently does not await the response). |
| `$/progress` | server → client notification | `{token,value}`; value kinds are `begin`, `report`, and `end`. |
| `window/workDoneProgress/cancel` | client → server notification | `{token}`; cancel the matching active operation. |
| `window/showMessage` | server → client notification | `{type,message}` standard LSP message. |

The implemented and advertised standard providers are exactly `textDocument/documentSymbol` (document outline), `textDocument/definition` (tree-backed definition/navigation), `textDocument/documentLink` (links; no resolve), `textDocument/foldingRange`, and `textDocument/semanticTokens/full` (full only; no range or delta). Requests identify a document by URI. Positional lookups use the acknowledged tree, falling back to the current tree before the first acknowledgement; they return `null` in text mode. Draft documents use full sync `{openClose:true, change:1}`. The semantic-token legend includes custom `tokenTypeGroups`. No hover, references, formatting, document-link resolve, semantic-token range or delta providers are implemented. Optional providers are not prerequisites for basic status/staging. See `src/server.rs:1371-1431,1883-1926` and `src/session.rs:1581-1590`.

## Descriptors, URIs and view lifetime

`ViewDescriptor` carries server-issued identity, kind, URI, title, placement, mode, scope and capture state (`src/view.rs`). Treat the URI as opaque; retain the descriptor and associate it with the server view ID. Placement is a hint, not a command to create a particular native layout. `view/close` closes one view; push/pop effects describe stack behavior but clients still decide concrete buffers/panels/windows. Do not assume replacing one window handles every lifetime or companion workflow.

Render/application ordering:

   1. On open, retain the descriptor and await/apply the initial render. Render and keymap notifications have separate bookkeeping; ordinary keymap notices use the view render version, while draft keymaps use 0. A keymap notification is a complete replacement, not a render acknowledgement or a guaranteed newly incremented counter.
2. Apply a full render by replacing all text. No `baseVersion` is required for a full render.
   3. If `text` is present, it takes precedence: replace all text regardless of edits. Otherwise apply edits only if local content is exactly `baseVersion`; line ranges are half-open `[startLine,endLine)`. The current producer emits zero or one prefix/suffix replacement. Both bundled clients apply supplied edits bottom-up by descending `startLine`; arbitrary multiple/overlapping edit ordering semantics are not defined.
4. Render text lines verbatim. A metadata-only render has neither `text` nor `edits`: preserve text and version, apply its mode/pending/menu/cursor metadata, then acknowledge application. Apply metadata accompanying content renders as well. Only after application send `view/rendered` for that version.
5. Server retains only current and last acknowledged trees. Positional commands must name the displayed version; stale or omitted positional versions are refused. Commit selection confirm/toggle-mark requires exact acknowledged version. Non-positional commands ignore version.
6. If a base is unavailable, update is lost, or local state cannot be proven to match, request `view/resync`; replace with returned full render, then ack. Never ack before application.

Selections use inclusive document line endpoints (`startLine` through `endLine`); line edits use exclusive ends. Cursor is zero-based. Pass structured cursor/selection and editor mode, never derive them by parsing display lines (`src/protocol.rs`, `src/tree.rs`, `src/session.rs`).

## Input, navigation and presentation

Canonical keys use `[modifier+]*key`, lowercase modifiers in `ctrl`, `alt`, `shift`, `meta` order and W3C key names; chord steps are sent individually. `dynamic` consumes complete `view/keymap` replacements atomically. `static` ships generated mappings and requires synchronization with scope/mode/capture. `forwardAll` sends every step and must execute ordinary editor behavior on `passThrough`. These are alternatives, not server feature negotiation. Terminal modifier loss, VS Code layout/IME/dead-key limits, and reserved editor shortcuts constrain capture. Do not intercept native behavior outside the active view/editor focus.

Text mode is disposable local plain-text editing/copying, not a Git edit or managed draft: local changes are not sent to the server tree, synchronized to Git, or preserved when the server replaces the document. Interactive mode is read-only server-rendered display. On entry the server sends a metadata-only render at the unchanged content/version; preserve the text, apply the mode, and acknowledge it (see render rules above). Do not require a native `view/setMode` control: mode changes produced by enabled server keys still arrive in render metadata and must be honored.

In text mode capture is off and only `common.toggle-text-mode` remains bound; other `view/invoke`/`view/key` commands are refused with a readable informational hint, and positional standard-provider requests return `null`. Refresh requests are deferred/queued until exit. On exit, the server applies any queued refresh and sends a full replacement because locally edited text has no shared diff base; discard the local edits and restore read-only interactive display. Never send those edits as managed-draft `didChange` content. Neovim makes the existing buffer modifiable and restores its undo level on entry, then clears undo history and locks it on exit (`lua/magit-lsp/buffer.lua:130-144`). VS Code's virtual content provider remains read-only and queues refreshes (`clients/vscode/lib/session.js:65-72`); do not claim editable text mode there. This is not a mandatory native editable-core capability. Server navigation uses tree metadata; native cursor movement/search may remain available where capture permits. Fold behavior is client-specific and coordinated with server folding, not inferred from text. Styling may use semantic tokens where actually advertised; do not require a particular theme/color.

## Prompts, effects and drafts

### Work-done progress and standard messages

The server emits progress for core key/stage operations. It sends `window/workDoneProgress/create` with a token, then `$/progress` begin (currently title `Git`, operation in `message`, cancellable true), reports as needed, and finally end. Current server quirk: initialize inserts `capabilities.window.workDoneProgress:true` into its result and sends create followed immediately by begin without waiting for the create response or negotiating client support (`src/server.rs:294-343,1903-1914`). A client using an LSP library should let it service standard requests/notifications; a manual transport must correlate and answer create by its JSON-RPC ID and dispatch notifications in both directions. `window/showMessage` is server-to-client `{type,message}` and is used for startup/configuration errors (`src/server.rs:1875-1880,250,824`).

Cancellation is a client-to-server `window/workDoneProgress/cancel` notification carrying the active `{token}`. For example, after receiving begin, send `{"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":"magit-lsp/mutation/1"}}`. The server reports `$/progress` with `value:{"kind":"report","message":"Cancelling…","cancellable":false}` and a separate informational effect, but keeps the operation reservation until its owner reaches terminal end; end alone does not state success. Operation outcomes/errors are delivered separately through effects (`src/server.rs:370-398,1741-1753`). Keep servicing prompt responses and progress-cancel notifications while an operation is outstanding; do not queue them behind that operation's completion. A rich progress UI is optional: correctly servicing this protocol traffic is not.

Prompt kinds are `confirm` (boolean), `text` (string), `choice` (`{id}` or `{custom}`), and `multiChoice` (`{ids:[...]}`). All responses include `cancelled`; cancellation has no value requirement. Honor labels/defaults, choice item IDs and secret masking/no-history behavior where applicable (`src/protocol.rs`, `lua/magit-lsp/prompt.lua`, `clients/vscode/lib/prompt.js`).

Effects serialize with camelCase `type` discriminators, e.g. `openView`, `pushView`, `popView`, `focusView`, `visitFile`, `editDocument`, `passThrough`, `clipboard`, `message`, `quit`—not Rust enum variant names. Messages carry level/text and optional hint/detail/processLogId. Visit-file uses a resolved file URI and zero-based location. Rendering is not an effect. Implement each effect used by enabled commands with readable errors.

Managed drafts are an optional workflow, not a core requirement. `editDocument` identifies the opaque `magit-commit://` URI and carries `mode:"managedDraft"`, `runId`, and initial text/version. Open it as a full-sync document (`textDocument/didOpen` version 1); every `didChange` sends exactly one full-text content change, with monotonically increasing positive i32 versions and UTF-8 text no larger than 1 MiB. Save is `draft/save {runId,uri,version}`; history is `draft/history {runId,uri,version,command}`, where command is `commit-message/previous` or `commit-message/next`; finish is `draft/finish {runId,uri,version,reason}`, with reason `confirm` or `normal-close`. These are no-text barriers: synchronize the full text first, then pin the run/URI/version and gate edits until the response. Save returns `draft/checkpointed`; finish returns `draft/finished`; history may return `draft/replaced {baseVersion,version,text}`. Apply replacement only if baseVersion still matches, then suppress the replacement's didChange echo. On opening a run, use the exact supplied text/version; do not synthesize an initial change. See `src/protocol.rs:60-159`, `src/server.rs:1169-1213`, `src/editor/draft.rs:155-251,310-316`.

Correlate each barrier by its JSON-RPC request ID. A rejected full sync can also emit `draft/error {uri,version,method,message,recoverable}`; separately, barrier validation failures are JSON-RPC `InvalidParams` errors whose `data` contains `{runId,uri,version,recoverable}`. Present readable errors, retain the user's local text, invalidate in-flight barriers after a rejected sync, and do not issue finish until synchronization is recovered. `recoverable:true` does not authorize reviving a terminal run. A finish response means editor writeback/helper release, not Git commit success: later effects/progress report the Git operation outcome (`src/server.rs:858-874,919-926,1149-1158,1808-1863`).

Abort explicitly through `view/invoke` on the owning ordinary view with `command:"common.cancel"` and `args:{runId}`; there is no `draft/abort` method, and `textDocument/didClose` only detaches. On normal close, clients choose the saved checkpoint; explicit abort, progress cancellation, disconnect/EOF/shutdown cancel. Neovim classifies intercepted close commands and cancels ambiguous unload/hidden/API-close paths (`lua/magit-lsp/edit.lua:135-188,340-368`). VS Code's explicit close command arbitrates Save/Discard/Cancel, while ambiguous tab/document teardown cancels (`clients/vscode/lib/drafts.js:529-598,819-887`; `clients/vscode/lib/edit.js`). These are observed client differences, not a guarantee that arbitrary native close intent can be inferred: the server bridge is currently Unix-only, with no claim of platform acceptance or tested parity. Drafts are in-memory, not crash durable; late responses cannot resurrect terminal runs.

## Feature-to-capability mapping and clients

| Workflow | Required client behavior | Present implementation / gap |
|---|---|---|
| Status, menus, stage/unstage | Core view/render/key or invoke, structured context, messages/prompts/effects used | Both clients implement status rendering and command routing (`lua/magit-lsp/view.lua`, `keymap.lua`, `keys.lua`; VS Code `render.js`, `views.js`, `requests.js`, `commands.js`). |
| Commit and managed draft history | Managed editor, full sync, exact save/finish/history barriers, close-intent arbitration | Neovim implemented in `edit.lua`; VS Code code exists in `drafts.js`/`edit.js`, verify current status before claiming complete. |
| Log selection / commit picker | Rendered log, selection mode, structured exact-acked commit node; no generic choice picker substitute | Server resolves selection; client forwards input and displays view. |
| Process log | Message detail/process-log handling and view opening if enabled | Optional workflow; do not assume every message includes an ID. |
| File visiting | Handle `visitFile` effect and open resolved URI/location | Optional; not a client-side Git lookup. |
| Visual selections | Send inclusive selection lines with cursor/mode and displayed version | Optional editor-specific interaction; server handles patch selection. |
| Splits/companion views | Multiple descriptor lifetimes and placement effects | Optional; placement fallback is client-specific, never universal single-window assumption. |

Neovim primarily uses dynamic keymaps and buffer-local capture; VS Code uses static contributed bindings plus context/focus guards. Both should remain presentation/input adapters. Consult the listed client sources and `src/session.rs` for actual behavior rather than extrapolating parity from this table.

## Minimal example

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{"keymapDelivery":"dynamic","keyModifiers":"full"}}}
{"jsonrpc":"2.0","id":2,"method":"view/open","params":{"kind":"status","args":{"repo_path":"/work/project"},"viewport":{"cols":80,"rows":24}}}
```

The open response is a descriptor. A subsequent server notification may be:

```json
{"jsonrpc":"2.0","method":"view/render","params":{"viewId":"view-1","version":1,"text":["On branch main",""],"cursor":{"line":0,"column":0},"mode":"interactive"}}
```

Apply exact lines, then notify `view/rendered` with version 1. A key request then includes that displayed version and structured cursor; apply resulting render/effects in order and ack each applied render. IDs and text are illustrative.

## Implementation checklist and unresolved decisions

- [ ] Implement standard lifecycle and only actual advertised standard providers.
- [ ] Open status, retain descriptor/opaque URI, verbatim apply, ack after apply, resync on loss.
- [ ] Choose dynamic/static/forwardAll and document capture limitations; preserve native pass-through.
- [ ] Forward structured cursor, inclusive selection, editor mode and displayed version.
- [ ] Implement prompt kinds and effects required by the enabled command set.
- [ ] Add resize/text mode and multiple placements only where needed.
- [ ] Implement managed drafts only with ordered full sync, barriers, history replacement and explicit close-intent semantics.

Unresolved/follow-up: define support expectations for external clients' optional effects; decide whether any future method-level capability discovery is needed (none currently); document interoperability expectations for clients that cannot distinguish intentional close from teardown; decide whether multi-view placement needs stronger client lifecycle guidance. These are not current negotiation mechanisms.

**Source cross-check:** `src/protocol.rs` (wire types/serialization), `src/server.rs` (dispatch/lifecycle), `src/view.rs` (descriptor), `src/session.rs` (render/ack retention); Neovim `lua/magit-lsp/init.lua`, `view.lua`, `buffer.lua`, `requests.lua`, `keymap.lua`, `keys.lua`, `prompt.lua`, `effects.lua`, `edit.lua`; VS Code `clients/vscode/lib/session.js`, `requests.js`, `render.js`, `views.js`, `commands.js`, `prompt.js`, `effects.js`, `drafts.js`, `edit.js`. Re-check these sources as contracts evolve.
