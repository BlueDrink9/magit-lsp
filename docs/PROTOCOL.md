## 4. Protocol reference

No backward compatibility: delete the old `vcs/*` methods as their
replacements land.

### 4.1 Client → server (current contract)

Initialize selects Plain when `capabilities.experimental.magit` is absent. A declared Rich v1 capability object is strictly validated. Effective capability and quirk reporting already exists; T005 is complete through its authoritative [Ad hoc children](../TASKS.md#completed-work). Wire decisions are recorded in the [rich protocol v1 decision record](design/rich-protocol-v1.md); the version handshake below is implemented in the server and both bundled clients.

| Method | Params | Returns |
| --- | --- | --- |
| `initialize` | `clientInfo`; rich clients declare `capabilities.experimental.magit` with `v` and optional `minimumVersion`. Absence selects Plain. `profile?` remains in initializationOptions. | server capabilities, including `experimental.magit.protocolVersion` |
| `view/open` | `{kind, args?, placement?, viewport}` (`status` carries `args.repo_path`) | `ViewDescriptor` |
| `view/close` | `{viewId}` | — |
| `view/key` | `{viewId, version, key, editorMode, cursor: {line, column}, selection?: {startLine, endLine}}` | `{effects: [...]}` |
| `view/invoke` | `{viewId, version?, command, args?, cursor?, selection?}` | `{effects: [...]}` |
| `view/rendered` (notification) | `{viewId, version}` | — |
| `view/resync` | `{viewId}` | full render |
| `view/resize` (notification) | `{viewId, cols, rows}` | — |
| `view/setMode` | `{viewId, mode: "interactive"/"text"}` | `{effects}` |
| `view/promptRespond` | `{promptId, cancelled, value?}` | — |

Standard positional reads (`documentSymbol`, `definition`, `documentLink`,
`foldingRange`, semantic tokens) resolve against the last acknowledged tree, or
the current tree before the first acknowledgement, and return `null` in text
mode. Rich `view/invoke` and `view/key` commands are different: `version`
selects the exact acknowledged version or, under the acknowledgement and
validated-offer rules, the exact current version; a stale version produces the
readable stale-view warning. The two policies are deliberate; see the decision
record.

**Version negotiation (implemented; T113 decision).** The initialize result
reports `capabilities.experimental.magit.protocolVersion`, the version of every
accepted rich session (currently `1`), not a maximum. A rich declaration
carries `v`, the single session version it selects, and optional
`minimumVersion`, the oldest server session version it can use; `minimumVersion`
defaults to `v`, and a present value must be a positive integer no greater than
`v` (a present `null` is rejected). The server accepts a session only when
`minimumVersion <= v == protocolVersion`, rejects `v != 1` with the existing
readable error, and rejects a malformed or out-of-range `minimumVersion` with a
readable initialize error; the out-of-range error names both numbers. It never
clamps, downgrades, retries or substitutes a version. Both bundled clients declare `v = 1` and
`minimumVersion = 1`; each requires a present, positive-integer advertisement
equal to its declared version. A missing, malformed or different advertisement
shows a readable required/advertised-version message, opens no rich view and
sends no rich traffic; neither client treats the advertisement as `1` or falls
back to Plain. Neovim also stops the transport on that failure. Initialize
rejections reach the user through the language client's start error (VS Code)
or a failed-client notification (Neovim); post-initialize failures use each
client's existing notification channel. The [decision record](design/rich-protocol-v1.md)
tabulates every outcome.

### 4.2 Server → client

| Method | Params |
| --- | --- |
| `view/render` (notification) | `{viewId, version, baseVersion?, text?, edits?: [{startLine, endLine, lines}], cursor?: {line, column}, mode, pending?, menu?: Menu}` |
| `view/keymap` (notification) | `{viewId, scope, version, capture, bindings: [{command, keys: [string], modes: ["normal","visual"], label, status}], mode, pending?, liveKeys?: [string]}` |
| `view/effects` (notification) | `{effects: [...]}` for asynchronous effects |
| `view/prompt` (notification) | See [Architecture §3.6](ARCHITECTURE.md#36-prompts-what-each-kind-means) and the [detailed client contract](CLIENT_SPEC.md); the client answers with the `view/promptRespond` request (`{promptId, cancelled, value?}`) |
| standard server requests/notifications | `window/showDocument`, `window/showMessage`, `$/progress` |

`view/render` has three forms: full `text` (no `edits`), a present `edits`
array with zero or more non-overlapping edits against `baseVersion`, or a
state/mode-only render with neither `text` nor `edits` that does not advance
`version`. An empty `edits` array is the incremental form with nothing to
change: an acknowledged refresh whose text is unchanged sends `edits: []` with a
base and a new version, so the client validates the base, retains its text, and
adopts and acknowledges the delivered version. Only the absence of both `text`
and `edits` is mode/state-only. The server currently emits at most one edit; if
a future server emits several, they do not overlap, `endLine` is exclusive, and
clients apply them bottom-up (descending `startLine`).

A view retains one acknowledged tree/version, which is the edit base and the
source for standard positional reads. Successive renders may be delivered before
any acknowledgement; a render whose base is not the retained acknowledged
version carries full `text`. An acknowledgement is valid only for the current
version; stale acknowledgements are no-ops. A client that missed an update
requests `view/resync` instead of acknowledging an older version.

`view/keymap.version` is the advisory render generation the bindings belong to;
it is never acknowledged. `0` identifies a draft/edit keymap keyed by `runId`,
not a normal view. A static client may be unable to replay a `passThrough` key
that is neither printable nor a clipboard shortcut; it logs that client limit,
and declaring basic modifier fidelity does not change replay capability.

Current shared Rich/Plain providers are document symbols, definition, document
links (no resolve), folding ranges, full semantic tokens, code actions, and the
`magit.command` execute command; document synchronization is also advertised.
There are no completion, hover, references, rename, or diagnostics providers.
Plain code actions/commands dispatch semantic actions over standard LSP; they
are current behavior, not future targets. Plain completion candidates, Git
discovery, mutation validation, commit-file paths, and helper lifecycle remain
provider/workflow targets, not advertised LSP providers. Broader Plain dispatch
primitives remain target work; see [CLIENT_SPEC](CLIENT_SPEC.md#client-implementation-levels).

Plain mode uses standard document synchronization and implemented code
actions/commands for semantic action dispatch; concrete views/data carry stable identities and are
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
