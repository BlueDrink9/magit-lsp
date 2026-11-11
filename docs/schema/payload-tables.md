#### Protocol methods

| Method | Kind | Direction | Params | Result |
|---|---|---|---|---|
| `view/open` | request | clientToServer | `ViewOpenParams` (`view-open-params.json`) | `ViewDescriptor` (`view-descriptor.json`) |
| `view/close` | request | clientToServer | `ViewCloseParams` (`view-close-params.json`) | unconstrained JSON |
| `view/invoke` | request | clientToServer | `ViewInvokeParams` (`view-invoke-params.json`) | `ViewInvokeResponse` (`view-invoke-response.json`) |
| `view/key` | request | clientToServer | `ViewKeyParams` (`view-key-params.json`) | `ViewInvokeResponse` (`view-invoke-response.json`) |
| `view/setMode` | request | clientToServer | `ViewSetModeParams` (`view-set-mode-params.json`) | `ViewInvokeResponse` (`view-invoke-response.json`) |
| `view/resync` | request | clientToServer | `ViewResyncParams` (`view-resync-params.json`) | `ViewRender` (`view-render.json`) |
| `view/promptRespond` | request | clientToServer | `ViewPromptRespondParams` (`view-prompt-respond-params.json`) | unconstrained JSON |
| `view/pickRespond` | request | clientToServer | `ViewPickRespondParams` (`view-pick-respond-params.json`) | unconstrained JSON |
| `view/render` | notification | serverToClient | `ViewRender` (`view-render.json`) | — |
| `view/effects` | notification | serverToClient | `ViewEffectsParams` (`view-effects-params.json`) | — |
| `view/keymap` | notification | serverToClient | `ViewKeymapParams` (`view-keymap-params.json`) | — |
| `view/prompt` | notification | serverToClient | `ViewPromptParams` (`view-prompt-params.json`) | — |
| `view/pick` | notification | serverToClient | `ViewPickParams` (`view-pick-params.json`) | — |
| `view/rendered` | notification | clientToServer | `ViewRenderedParams` (`view-rendered-params.json`) | — |
| `view/resize` | notification | clientToServer | `ViewResizeParams` (`view-resize-params.json`) | — |

#### Payload schemas

##### `EditAbortParams` (`edit-abort-params.json`)

Run-aware abort intent for an attached commit draft.

| Field | Type | Required | Notes |
|---|---|---|---|
| `command` | string | required | Semantic command; only common.cancel is accepted by this dispatcher. |
| `runId` | string | required | Server-issued run identity. |
| `uri` | string | required | Exact server-issued draft URI. |
| `version` | integer | required | Exact open synchronized document version. |

##### `EditAbortResult` (`edit-abort-result.json`)

Result of aborting a pending editor run, not a commit confirmation.

**`aborted`** — The pending editor run was aborted.

| Field | Type | Required | Notes |
|---|---|---|---|
| `runId` | string | required |  |
| `type` | string | required | Literal `"aborted"` |
| `uri` | string | required |  |
| `version` | integer | required |  |

##### `EditConfirmParams` (`edit-confirm-params.json`)

Run-aware semantic confirmation routed through `magit.command`.

| Field | Type | Required | Notes |
|---|---|---|---|
| `command` | string | required | Semantic command; only common.confirm is accepted by this dispatcher. |
| `runId` | string | required | Server-issued run, protecting a reused COMMIT_EDITMSG URI. |
| `source` | EditConfirmSource | required | Select synchronized writeback versus independently read saved disk text.; The two intentionally distinct commit confirmation sources.; One of `"synchronized"`, `"saved-file"` |
| `uri` | string | required | Exact server-issued canonical file URI. |
| `version` | integer | required | Exact synchronized version, or the close event's last observed version. |

##### `EditConfirmation` (`edit-confirmation.json`)

Semantic executeCommand confirmation, returned after writeback/helper release.

**`confirmed`** — The selected content source was accepted and the helper released, not necessarily committed: Git still owns hooks and cleanup.

| Field | Type | Required | Notes |
|---|---|---|---|
| `runId` | string | required |  |
| `type` | string | required | Literal `"confirmed"` |
| `uri` | string | required |  |
| `version` | integer | required |  |

##### `Effect` (`effect.json`)

A user-facing consequence of a request.

**`message`** — Show a notification; `hint` and `detail` are optional extras.

| Field | Type | Required | Notes |
|---|---|---|---|
| `detail` | string \| null | optional |  |
| `hint` | string \| null | optional |  |
| `level` | MessageLevel | required | Severity of a message effect.; One of `"info"`, `"warn"`, `"error"` |
| `text` | string | required |  |
| `type` | string | required | Literal `"message"` |

**`passThrough`** — The key is not bound here; perform its normal editor behaviour.

| Field | Type | Required | Notes |
|---|---|---|---|
| `key` | string | required |  |
| `type` | string | required | Literal `"passThrough"` |

**`clipboard`** — Put `text` on the system clipboard.

| Field | Type | Required | Notes |
|---|---|---|---|
| `text` | string | required |  |
| `type` | string | required | Literal `"clipboard"` |

**`pushView`** — Create a window/buffer/panel and show it, remembering the previous view so `PopView` returns.

| Field | Type | Required | Notes |
|---|---|---|---|
| `descriptor` | ViewDescriptor | required | Everything a client needs to create the buffer for a view. |
| `type` | string | required | Literal `"pushView"` |

**`popView`** — Close that view and focus the previous one.

| Field | Type | Required | Notes |
|---|---|---|---|
| `type` | string | required | Literal `"popView"` |
| `viewId` | string | required |  |

**`focusView`** — Focus an existing view.

| Field | Type | Required | Notes |
|---|---|---|---|
| `type` | string | required | Literal `"focusView"` |
| `viewId` | string | required |  |

**`quit`** — Close every view of the session.

| Field | Type | Required | Notes |
|---|---|---|---|
| `type` | string | required | Literal `"quit"` |

**`visitFile`** — Open a worktree file at a location the server resolved from its tree. `uri` is a `file:` URI; `line` and `column` are 0-based.

| Field | Type | Required | Notes |
|---|---|---|---|
| `column` | integer | required |  |
| `line` | integer | required |  |
| `type` | string | required | Literal `"visitFile"` |
| `uri` | string | required |  |

**`editDocument`** — Open Git's real commit-message file, or re-present the same pending run after failed native close. New runs start at version 1; recovery carries the accepted snapshot and must not replace retained client edits/versions.

| Field | Type | Required | Notes |
|---|---|---|---|
| `kind` | string | required |  |
| `mode` | EditDocumentMode | required | How an editor-bridge document is edited.; One of `"realFile"` |
| `runId` | string | required |  |
| `scope` | string | required |  |
| `text` | string | required |  |
| `title` | string | required |  |
| `type` | string | required | Literal `"editDocument"` |
| `uri` | string | required |  |
| `version` | integer | required |  |

##### `Menu` (`menu.json`)

A menu: its definition plus the current switch/option state.

| Field | Type | Required | Notes |
|---|---|---|---|
| `groups` | Group[] | required |  |
| `id` | string | required |  |
| `savedArgs` | Args | required | The arguments `menu.reset-args` restores.; A saved set of switches and option values. |
| `title` | string | required |  |

##### `PlainActionParams` (`plain-action-params.json`)

A Plain `workspace/executeCommand` action offered from a retained render.

| Field | Type | Required | Notes |
|---|---|---|---|
| `command` | string | required | Registered command identifier. |
| `documentUri` | string | required | Open document URI the action was offered in. |
| `target` | PlainActionTarget | required | Typed target from the offering render.; A content mutation target or a non-mutating logical-view target. The wire object always carries a camelCase `kind` discriminator so clients can forward an offered target without inspecting its fields. |
| `viewGeneration` | integer | required | Render generation the action was offered from. |

##### `ViewCloseParams` (`view-close-params.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `viewId` | string | required |  |

##### `ViewDescriptor` (`view-descriptor.json`)

Everything a client needs to create the buffer for a view.

| Field | Type | Required | Notes |
|---|---|---|---|
| `capture` | boolean | required |  |
| `id` | string | required |  |
| `kind` | string | required | The wire name; schemars models it as an arbitrary string because `ViewKind` has custom serde. |
| `mode` | ViewMode | required | Whether the client may edit the view or only display it.; One of `"interactive"`, `"text"` |
| `placement` | Placement | required | Where the server suggests the client put a view.; One of `"main"`, `"side"`, `"bottom"`, `"float"` |
| `scope` | Scope | required | A named group of key bindings, e.g. `common` or `status`. |
| `title` | string | required |  |
| `uri` | string | required |  |
| `viewport` | Viewport | required | keep-for: TASKS §3.2 records viewport in the descriptor; clients may use it when creating the view.; The client's text area, needed to lay out multi-column views. |

##### `ViewEffectsParams` (`view-effects-params.json`)

`view/effects`: effects produced after a request already answered, such as an `editDocument` from a detached Git process.

| Field | Type | Required | Notes |
|---|---|---|---|
| `effects` | Effect[] | required |  |

##### `ViewInvokeParams` (`view-invoke-params.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `args` | any | optional | Unconstrained JSON |
| `command` | string | required |  |
| `cursor` | Cursor \| null | optional | A cursor position in the rendered view, 0-based. |
| `selection` | Selection \| null | optional | An inclusive document-line selection, as produced by visual mode. |
| `version` | integer \| null | optional | Displayed render version; required for commands consuming cursor/selection. Only the current or last acknowledged tree is retained. Other versions (including an omitted version) produce a readable stale-view refusal. Commit selection (`common.confirm` in a selection view and `select.toggle-mark`) requires the exact acknowledged version. Non-positional commands ignore this field. |
| `viewId` | string | required |  |

##### `ViewInvokeResponse` (`view-invoke-response.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `effects` | Effect[] | required |  |

##### `ViewKeyParams` (`view-key-params.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `cursor` | Cursor | required | A cursor position in the rendered view, 0-based. |
| `editorMode` | EditorMode | required | The editor mode a binding is active in.; One of `"normal"`, `"visual"` |
| `key` | string | required | One canonical key step, e.g. `s`, `g`, `ctrl+c` (§3.8). |
| `selection` | Selection \| null | optional | An inclusive document-line selection, as produced by visual mode. |
| `version` | integer | required | Displayed render version for positional commands, with the same retained tree policy as `view/invoke`. Non-positional/menu keys ignore it. |
| `viewId` | string | required |  |

##### `ViewKeymapParams` (`view-keymap-params.json`)

`view/keymap`: the complete binding set for one view scope.

| Field | Type | Required | Notes |
|---|---|---|---|
| `bindings` | Binding[] | required |  |
| `capture` | boolean | required |  |
| `liveKeys` | string[] | optional | The keys that do something right now, so clients need no chord logic. |
| `mode` | ViewMode | required | The view mode these bindings belong to (`text` has only the toggle).; Whether the client may edit the view or only display it.; One of `"interactive"`, `"text"` |
| `pending` | string | optional | The menu steps typed so far, e.g. `-`; empty for other views. |
| `scope` | string | required |  |
| `version` | integer | required |  |
| `viewId` | string | required |  |

##### `ViewOpenParams` (`view-open-params.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `args` | any | optional | Kind-specific arguments. A status view carries `repo_path` here. |
| `kind` | string | required |  |
| `placement` | Placement \| null | optional | Where the server suggests the client put a view.; One of `"main"`, `"side"`, `"bottom"`, `"float"` |
| `viewport` | Viewport | required | The client's text area, needed to lay out multi-column views. |

##### `ViewPickParams` (`view-pick-params.json`)

`view/pick`: a typed selection request the server is waiting on.

| Field | Type | Required | Notes |
|---|---|---|---|
| `allowFreeText` | boolean | required | Whether the user may answer with text that is not an item id. |
| `fallbackView` | string \| null | optional | URI of a server-rendered view that offers the same choice when the native picker cannot present the request shape. |
| `items` | PickItem[] | required |  |
| `multi` | boolean | required | Whether more than one item may be selected. |
| `pickId` | string | required | The pick this answer refers back to. |
| `preselection` | string[] | required | Item ids the client should pre-select. |
| `prompt` | string | required | The question, shown prominently. |
| `purpose` | string | required | Why the pick exists, used by clients to choose a rung. |

##### `ViewPickRespondParams` (`view-pick-respond-params.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `cancelled` | boolean | required |  |
| `freeText` | string \| null | optional |  |
| `pickId` | string | required |  |
| `selected` | string[] | required |  |

##### `ViewPromptParams` (`view-prompt-params.json`)

`view/prompt`: one question the server is waiting on.

| Field | Type | Required | Notes |
|---|---|---|---|
| `allowCustom` | boolean \| null | optional |  |
| `default` | any | optional | Kind-dependent default: a bool, a string or an item id. |
| `destructive` | boolean \| null | optional |  |
| `detail` | string \| null | optional |  |
| `items` | array \| null | optional |  |
| `kind` | PromptKind | required | The four kinds of question the server can ask (§3.6).; One of `"confirm"`, `"text"`, `"choice"`, `"multiChoice"` |
| `max` | integer \| null | optional |  |
| `min` | integer \| null | optional |  |
| `noLabel` | string \| null | optional |  |
| `placeholder` | string \| null | optional |  |
| `preselected` | array \| null | optional |  |
| `promptId` | string | required |  |
| `secret` | boolean \| null | optional |  |
| `title` | string | required | The question, shown prominently. |
| `yesLabel` | string \| null | optional |  |

##### `ViewPromptRespondParams` (`view-prompt-respond-params.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `cancelled` | boolean | required |  |
| `promptId` | string | required |  |
| `value` | PromptValue \| null | optional | The value of a prompt answer; its shape follows the prompt kind. |

##### `ViewRender` (`view-render.json`)

`view/render`: a full document or a line-range update.

| Field | Type | Required | Notes |
|---|---|---|---|
| `baseVersion` | integer \| null | optional |  |
| `cursor` | Cursor \| null | optional | A cursor position in the rendered view, 0-based. |
| `edits` | array \| null | optional |  |
| `menu` | Menu \| null | optional | The structured menu model, for menu views only (§3.11).; A menu: its definition plus the current switch/option state. |
| `mode` | ViewMode | required | The view mode this render belongs to, so the client can show it.; Whether the client may edit the view or only display it.; One of `"interactive"`, `"text"` |
| `pending` | string | optional | The menu steps typed so far, e.g. `-` or `g`; empty otherwise. |
| `text` | array \| null | optional |  |
| `version` | integer | required |  |
| `viewId` | string | required |  |

##### `ViewRenderedParams` (`view-rendered-params.json`)

`view/rendered`: the client applied a version.

| Field | Type | Required | Notes |
|---|---|---|---|
| `version` | integer | required |  |
| `viewId` | string | required |  |

##### `ViewResizeParams` (`view-resize-params.json`)

`view/resize`: the client's text area changed.

| Field | Type | Required | Notes |
|---|---|---|---|
| `cols` | integer | required |  |
| `rows` | integer | required |  |
| `viewId` | string | required |  |

##### `ViewResyncParams` (`view-resync-params.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `viewId` | string | required |  |

##### `ViewSetModeParams` (`view-set-mode-params.json`)

| Field | Type | Required | Notes |
|---|---|---|---|
| `mode` | ViewMode | required | Whether the client may edit the view or only display it.; One of `"interactive"`, `"text"` |
| `viewId` | string | required |  |


