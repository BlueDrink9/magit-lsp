| `PlainCapabilities` field | Initialize capability | Plain level | Effect |
|---|---|---|---|
| `apply_edit` | `workspace.applyEdit` | L1 | Status document delivered as a standard `WorkspaceEdit`; status actions and commit drafts need it. |
| `document_changes` | `workspace.workspaceEdit.documentChanges` | L1 (versioned sub-capability) | Versioned `documentChanges` with the client's version; absent uses the legacy unversioned `changes` map. |
| `code_actions` | `textDocument/codeAction` | L2 | Code actions are returned; `magit.command` runs them without the client declaring `workspace/executeCommand`. |
| `disabled_actions` | `textDocument/codeAction.disabledSupport` | L2 (sub-capability) | Unavailable and planned actions carry a disabled reason; absent they are omitted. |
| `show_document` | `window/showDocument.support` | L3 | Commit drafts open through `window/showDocument`; absent sends manual instructions instead. |
| `work_done_progress` | `window/workDoneProgress` | L3 | Mutations run `window/workDoneProgress/create` and `$/progress` begin/report/end; absent still runs with messages. |
| `completion` | `textDocument/completion` | none (not implemented) | Extracted into the effective-capability echo only; the server advertises no provider and registers no handler. |
| `folding_range` | `textDocument/foldingRange` | L3 (request-only) | The client may request server-owned folds; declaring it gates no lower level. |
| `position_encodings` | `general.positionEncodings` | L0 | Negotiates `utf-8`, `utf-16` or `utf-32`; absent defaults to `utf-16`. |

