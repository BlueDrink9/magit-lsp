# External client implementation specification

Rust wire definitions and handlers are authoritative; this guide is cross-checked against both bundled clients. This is an implementation guide, not a promise that every optional workflow is supported by every client.

## Capability mapping

| Capability | Status | Rationale | Unsupported behavior |
|---|---|---|---|
| Standard LSP lifecycle; `view/open`, render display, `view/rendered`, `view/resync` | REQUIRED | Core session and verbatim status view with ordered application and recovery. | No core view. |
| `view/key` or `view/invoke`, structured cursor/selection/version | REQUIRED | Core menu, navigation, stage/unstage actions. | Core flow unavailable; never infer structure from text. |
| Prompts/effects | REQUIRED for enabled prompting/effect-producing operations | Confirm, collect input, report messages and apply view effects. | Disable affected operations or provide equivalent interaction; never guess. |
| `view/resize`, `view/setMode` | RECOMMENDED | Width-sensitive layout and interactive/text toggle. | Default viewport/layout; remain interactive. |
| Dynamic keymaps | RECOMMENDED; `static` and `forwardAll` are alternatives | Use server mappings or forward keys and honor pass-through. | Static bindings or forward every key and execute normal editor behavior on `PassThrough`. |
| Managed-draft bridge | RECOMMENDED; required for draft editing/history workflows | Versioned server-owned commit-message document. | Disable workflow; never edit a Git path as substitute. |
| Companion/split placements | RECOMMENDED only when requested by a workflow descriptor | Present related views conveniently. | Fall back to one replaceable view; no universal split requirement. |

Preserve server-owned text and structured actions/navigation. Display render lines verbatim; use tree-derived cursor, selection and versions as context. There is no separate structure notification in the current contract (`src/tree.rs`, `src/protocol.rs`).

## Wire contract

Initialize using standard LSP. Optional initialization capabilities `keymapDelivery` accept `dynamic` (default), `static`, `forwardAll`; `keyModifiers` accept `full` (default) or `basic` (`src/protocol.rs`: `KeymapDelivery`, `KeyModifiers`). These describe client preferences, not negotiation of arbitrary methods. VS Code supplies `profile` in initialization options.

`view/open` request parameters: `kind`, optional `args`, `placement`, `viewport`; result `ViewDescriptor`. Status `args.repo_path` identifies the repository. `view/close` takes `viewId`. Server notification `view/render` carries `viewId`, `version`, full `text` or `edits` plus `baseVersion`, and optional cursor, mode, pending prefix and menu. Apply updates in order, verbatim, then notify `view/rendered` with applied version. If base is missing or updates are lost, request `view/resync` and replace with returned full render. Never ack before apply. Client `view/resize` reports cols/rows.

`view/invoke` accepts viewId, command, optional args/cursor/selection/version; `view/key` accepts viewId, version, one canonical key, editorMode, cursor and optional selection. Positional commands require the displayed version; stale/omitted versions are refused. Non-positional commands ignore it. `view/keymap` sends complete binding scope/version, capture, mode, pending and live keys. Dynamic clients apply maps atomically; static clients ship bindings; forward-all clients forward individual canonical steps and perform ordinary editor behavior when result effects include `PassThrough`. `view/setMode` changes mode.

`view/prompt` notification provides promptId, kind, title and kind-specific options; answer with `view/promptRespond` request containing promptId, cancelled and optional kind-shaped value. Support confirm/text/choice/multiChoice, defaults and cancellation for enabled workflows. Effects appear in invoke results or asynchronous `view/effects`: messages, clipboard, view open/push/pop/focus/quit, visit-file and edit-document. Implement relevant variants with readable user-facing errors.

Managed drafts are not editable repository paths. `EditDocument` carries runId, opaque URI, text/version, kind/title/mode/scope. Send full-sync document changes. `draft/save` checkpoints exact applied version; `draft/finish` names saved version and reason; `draft/history` identifies applied version and server command. Apply `draft/replaced` only to its matching base and serialize editing while waiting. `didClose` detaches, it is not finish intent. See `src/protocol.rs` draft types and `src/server.rs` handlers.

## Example: open, render, acknowledge

```json
{"jsonrpc":"2.0","id":1,"method":"view/open","params":{"kind":"status","args":{"repo_path":"/work/project"},"viewport":{"cols":80,"rows":24}}}
```

The response is a `ViewDescriptor`; server notification:

```json
{"jsonrpc":"2.0","method":"view/render","params":{"viewId":"view-1","version":1,"text":["On branch main",""],"cursor":{"line":0,"column":0},"mode":"interactive"}}
```

After applying exact lines, send `{"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":"view-1","version":1}}`. IDs and enum values in this illustrative example are server-defined.

## Implementation checklist and evidence

1. Implement LSP lifecycle and choose key delivery preference.
2. Open status; retain descriptor; display verbatim; apply ordered full/incremental renders, ack, resync on loss.
3. Forward key steps or invoke commands with displayed version and structured position; honor pass-through/menu state.
4. Implement prompts/effects used by enabled commands; add resize/mode as appropriate.
5. Enable drafts/placements only after implementing their bridge/lifetime semantics.

Evidence: `src/protocol.rs` types/methods, `src/server.rs` dispatch, `src/view.rs` descriptors. Neovim: `lua/magit-lsp/init.lua`, `view.lua`, `keymap.lua`, `keys.lua`, `prompt.lua`, `effects.lua`, `edit.lua`. VS Code: `clients/vscode/lib/session.js` sets static/full; `render.js`, `views.js`, `requests.js`, `commands.js`, `prompt.js`, `effects.js`, `drafts.js`, `edit.js` implement corresponding paths. Placement fallback is client-specific; static maps require release synchronization. Clients must state unsupported prompt/effect kinds and disable dependent operations. Source contracts supersede this summary as they evolve.
