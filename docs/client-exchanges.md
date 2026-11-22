# Client exchanges

## Worked exchanges

Treat view and descriptor URIs as opaque. Cursor positions are zero-based, selections inclusive and line ranges half-open (end exclusive). `editorMode` is exactly `normal|visual` and a key is one canonical step. Dynamic edit keymaps identify `runId` as `viewId`, though edits are not ordinary `view/open` views; confirmation uses the semantic executeCommand above. Progress is separate LSP traffic, never an effect. Root view creation uses the `view/open` request and its returned descriptor; `pushView` adds a view to the session stack, and `popView` returns to the prior view. History commands are planned.

Presentation differs: Neovim displays rendered menu buffers; VS Code may present structured menu data via QuickPick. VS Code pass-through replay and placement are limited/concrete client behavior, not universal parity. Buffer render/keymap notifications until the descriptor/effect is registered. Progress notifications may be emitted before the request response; deferred view notifications follow the response and can arrive before client-side view registration. Dynamic keymaps are complete replacements; do not infer new render versions from them.

See the generated [worked-exchange index](schema/worked-exchanges.md) and its numbered transcript parts. The generated parts are the canonical recordings, produced from the maintained [source recordings](examples/); regenerating them through the schema targets is authoritative, and this page describes what each exchange demonstrates.

The recorded initialize results carry `capabilities.experimental.magit.protocolVersion = 1`; the rich example declares the required `v = 1` and omits the optional `minimumVersion` (absence means `v`). Frames recorded before later command-table regeneration may differ from current output in non-initialize details.

##### `commit-edit-abort.jsonl`

Stages one hunk, opens the commit menu and launches the commit, then cancels at the commit-message prompt: the cancelled prompt reports `aborted`, Git's own failure arrives as a `window/showMessage`, and the session shuts down.

See the generated [commit-edit-abort transcript](schema/worked-exchanges-01.md#commit-edit-abortjsonl).

##### `commit-edit-confirm.jsonl`

The same staging and commit-menu exchange, but the commit-message prompt is confirmed: the server commits the recorded message, reports `Committed: …` and the status view settles on a clean working tree.

See the generated [commit-edit-confirm transcript](schema/worked-exchanges-02.md#commit-edit-confirmjsonl).

##### `plain-code-action.jsonl`

A Plain client asks for code actions on an unstaged hunk; the server returns them as `magit.command` actions, and staging through `workspace/executeCommand` updates the buffer with an edit.

See the generated [plain-code-action transcript](schema/worked-exchanges-03.md#plain-code-actionjsonl).

##### `plain-document-prompt.jsonl`

A Plain client discards a hunk; the server pushes a prompt view, returns the prompt's answers as code actions, and pops the prompt when the client answers `No`, leaving the unstaged diff in place.

See the generated [plain-document-prompt transcript](schema/worked-exchanges-03.md#plain-document-promptjsonl).

##### `plain-open.jsonl`

The minimal Plain lifecycle: a full initial render applied over an open document, with no dynamic keymap or progress traffic, followed by shutdown.

See the generated [plain-open transcript](schema/worked-exchanges-03.md#plain-openjsonl).

##### `rich-lifecycle.jsonl`

A Rich client declares effects, dynamic keys, layouts and prompts; it opens a status view, stages everything with a key (emitting progress create, begin and end), acknowledges the render and receives an incremental edit followed by a full re-render.

See the generated [rich-lifecycle transcript](schema/worked-exchanges-03.md#rich-lifecyclejsonl).


Progress creation and notifications are separate from the key result effects and occur for ordinary key requests. The recorded mutation key (`S`, `status.stage-all`) shows the server asking the client to create a progress token, with create, begin and end all preceding the key response; the server does not wait for the create response, so it may arrive after begin. A fast `g` refresh instead emits create only: the refresh finishes and progress ends before the client's create response marks the run visible, so no visible begin or end is reported. Progress end does not always precede the key response. Payload definitions are grounded in `src/view.rs`, `src/keymap.rs`, `src/protocol.rs`; presentation details in `lua/magit-lsp/view.lua`, `lua/magit-lsp/edit.lua`, `clients/vscode/lib/views.js`, `menus.js`, and `effects.js`.
