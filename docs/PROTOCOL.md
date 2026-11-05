## 4. Protocol reference

No backward compatibility: delete the old `vcs/*` methods as their
replacements land.

### 4.1 Client → server (current contract)

Initialize selects Plain when `capabilities.experimental.magit` is absent. A declared Rich v1 capability object is strictly validated. Effective capability and quirk reporting already exists; T005 is complete through its authoritative [Ad hoc children](../TASKS.md#ad-hoc).

| Method | Params | Returns |
| --- | --- | --- |
| `initialize` | `clientInfo`; rich clients declare `capabilities.experimental.magit` v1. Absence selects Plain. `profile?` remains in initializationOptions. | server capabilities |
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
| `view/prompt` (request) | See [Architecture §3.6](ARCHITECTURE.md#36-prompts-what-each-kind-means) and the [detailed client contract](CLIENT_SPEC.md); returns `{cancelled, value?}` |
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
jj backend or new wire protocol. Plain message editing reuses the implemented
real-file/ring contract in [Architecture §3.10](ARCHITECTURE.md#310-editor-bridge-open-gits-editor-inside-the-users-editor); broader Plain dispatch remains a target.

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
