## C. Current protocol (implemented vs design)

### Current contract (source-backed)

`src/protocol.rs` declares rich custom methods: client requests `view/open`, `view/close`, `view/invoke`, `view/key`, `view/setMode`, `view/resync`, `view/promptRespond`; server notifications `view/render`, `view/effects`, `view/keymap`, `view/prompt`; client notifications `view/rendered` and `view/resize`. Exact method strings and payload schemas appear in the complete source excerpt below. A render carries text or line edits, versions/base version, cursor, mode, pending menu prefix and structured menu. Render acknowledgements establish the retained base; positional invokes are refused unless they name a current or last-acknowledged render, while selection confirmation/toggle-mark require exact acknowledged version (`src/protocol.rs:179-201,265-282,293-313,392-411`). Cursor line/column are zero based; line selection endpoints inclusive; line-edit end exclusive (`155-177`).

Initialize negotiation is implemented in `src/presentation/mod.rs:33-75`: absent `capabilities.experimental.magit` selects Plain; present declaration must be an object with v=1 and supported enum/set values, otherwise initialize parsing errors. Plain extracts standard capabilities (`mod.rs:196-287`); content stage/unstage requires applyEdit+codeAction+executeCommand and Plain discard currently reports confirmation unavailable (`241-258`). Rich has core behavior plus declared optional effects/prompts; secret prompts additionally require `secret` (`141-194`). Prompt structs exist in protocol; availability is gated by presentation/client implementation, not implied solely by the structs. `view/promptRespond` transport is present. EditDocument/run-aware confirmation are distinct commit-editor protocol structures (`src/protocol.rs:60-140`), not generic prompt support.

`KeymapDelivery` supports dynamic (default), static and forwardAll; modifiers full (default) and basic (`src/protocol.rs:315-354`). Plain action target is an untagged choice among content, semantic-view, semantic-node and menu-item identity (`src/dispatch.rs:46-109`). Content mutation identity includes node id, mutation hash and optional inclusive selection. `DispatchRequest` binds command to optional displayed version/cursor/selection/key and optional content target (`src/dispatch.rs:111-137`). `offer_selection_target`, `selection_members`, `target_is_current`, `displayed_targets` are at `src/dispatch.rs:220-315`; names and locations are also indexed below. Standard position encodings are read from `general.positionEncodings` (`src/presentation/mod.rs:276-284`); server negotiated-document encoding path is `src/server.rs:2160-2184`.

### Plain bootstrap, content edits, and action envelope (implemented)

Plain mode is a standard-LSP adapter, not the rich `view/*` transport. On opening a Plain view the server requires `workspace.applyEdit`, renders the authoritative lines via `workspace/applyEdit`, and retains exact document snapshots. Each action is bound to a retained rendered snapshot and its view generation; server accepts its four-field execute-command argument `{command, documentUri, target, viewGeneration}` only when the URI/text snapshot, view identity and generation still match. The content target uses the literal wire keys `node_id`, `mutation_hash`, and optional inclusive `selection: {startLine, endLine}`. The other untagged target shapes are `{viewId}`, `{viewId, nodeId}`, and `{viewId, menuId, itemKey}`; `Content`, `View`, `Node`, and `MenuItem` are Rust variant names, not JSON discriminator fields. Exact evidence: parser `src/server.rs:1720-1732`, typed target types `src/dispatch.rs:9-109`, validity checks `src/server.rs:1766-1834`, plus the code-action producer excerpt below. This documents implementation, not completion of a general client-adapter milestone.

The current boundary is intentionally narrower than the rich protocol: Plain prompts/document-backed confirmation, general visual-selection UI/actions, a Plain commit-editor adapter, and Helix/client acceptance are not established by these types or code paths. In particular Plain `status.discard` is explicitly rejected pending confirmation (`src/presentation/mod.rs:241-258`); selection metadata can be represented in typed content targets, but this is not evidence of complete Plain selection affordances. Commit editing has real-file/run confirmation structures in `src/protocol.rs:60-140`, but those do not establish a Plain document prompt/commit adapter. Treat Helix as pending acceptance, not accepted by source-level negotiation. Rich prompt delivery is a separately declared capability, and Rich's `experimental.magit` declaration is strict v1 (`src/presentation/mod.rs:33-75,141-194`).

### Prompt interfaces and implementation inventory

There are two distinct UI traits. `presentation::Ui` is the typed render/effects/prompt delivery seam (`src/presentation/custom.rs:13-18`); its implementations are `CustomRender` (`custom.rs:86-104`), test-only `RecordingUi` (`custom.rs:113-147`), and a separate integration-test `RecordingUi` (`tests/plain_core.rs:67-92`). `PromptUi` is a separate input/answer abstraction (`src/ui.rs:16-21`): implementations are `NullUi` (`ui.rs:45-65`), `ScriptedUi` (`ui.rs:157-193`), and server adapter `ServerUi` (`src/server.rs:1058-1110`) implementing `PromptUi`, not `presentation::Ui`. `tests/plain_core.rs:9-51,94-137` tests rich prompt gating and typed recording; these tests do not demonstrate a Plain prompt client. Complete matching implementation inventory: `rg -n 'impl Ui for|impl PromptUi for|trait Ui|trait PromptUi' src tests` yields `src/presentation/custom.rs:13,86,116`, `src/ui.rs:16,45,157`, `src/server.rs:1075`, and `tests/plain_core.rs:70`.

Cargo wire serialization dependencies: `Cargo.toml:22: serde = { version = "1.0", features = ["derive"] }`; `Cargo.toml:23: serde_json = "1.0"`.

Numbered excerpts grounding Plain applyEdit snapshots, argument parsing, code actions, and input interfaces follow; the complete protocol/presentation source excerpts remain below and are not duplicated.

```text
src/server.rs:1720-1732
1720: /// Parse the Plain action shape from an `executeCommand` request.
1721: fn plain_execute_params(request: &Request) -> Option<crate::dispatch::PlainActionParams> {
1722:     let params: lsp_types::ExecuteCommandParams =
1723:         serde_json::from_value(request.params.clone()).ok()?;
1724:     if params.command != "magit.command" || params.arguments.len() != 1 {
1725:         return None;
1726:     }
1727:     let argument = &params.arguments[0];
1728:     if argument.get("documentUri").is_none() || argument.get("viewGeneration").is_none() {
1729:         return None;
1730:     }
1731:     serde_json::from_value(argument.clone()).ok()
1732: }

src/server.rs:1766-1785
1766:     let uri = params.document_uri.as_str();
1767:     let text = {
1768:         let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
1769:         match documents.get(uri) {
1770:             Some(document) if !document.repair => document.text.clone(),
1771:             _ => return plain_stale_message(),
1772:         }
1773:     };
1774:     let (repo, view_id, line, scope) = {
1775:         let documents = shared
1776:             .plain_documents
1777:             .lock()
1778:             .unwrap_or_else(|p| p.into_inner());
1779:         let Some(entry) = documents.get(uri) else { return plain_stale_message(); };
1780:         let Some((tree, view_id, generation, scope, menu)) = entry.controller.action_snapshot(&text)
1781:         else {
1782:             return plain_stale_message();
1783:         };
1784:         if view_id != entry.view_id || generation != params.view_generation {
1785:             return plain_stale_message();

src/server.rs:2390-2403
2390:                 "command": command.id,
2391:                 "documentUri": uri,
2392:                 "target": target,
2393:                 "viewGeneration": generation,
2394:             });
2395:             actions.push(serde_json::to_value(lsp_types::CodeAction {
2396:                 title: command.title.to_string(), kind: None, diagnostics: None, is_preferred: None,
2397:                 disabled: None, edit: None,
2398:                 command: Some(lsp_types::Command {
2399:                     title: command.title.to_string(), command: "magit.command".into(), arguments: Some(vec![args]),
2400:                 }),
2401:                 data: None,
2402:             }).unwrap_or(Value::Null));

src/presentation/custom.rs:13-22
13: pub trait Ui: Send + Sync {
14:     /// Build the existing wire notification for one rendered view.
15:     fn render(&self, render: ViewRender) -> Message;
16:     /// Build the existing wire notification for asynchronous effects.
17:     fn effects(&self, effects: ViewEffectsParams) -> Message;
18:     /// Build the prompt notification; the caller owns its response lifecycle.
19:     fn prompt(&self, prompt: ViewPromptParams) -> Message;
20: }

src/ui.rs:16-29
16: pub trait PromptUi {
17:     /// Ask a yes/no question, returning the answer.
18:     fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>>;
19:
20:     /// Ask for one line of text.
21:     fn text(&self, request: &ViewPromptParams) -> Result<Option<String>>;
22:
23:     /// Pick exactly one item (or a custom value when allowed).
24:     fn choice(&self, request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>>;
25:
26:     /// Pick zero or more items by id.
27:     /// keep-for: TASKS §3.6/R-3.1 generic prompt-kind contract; no current command emits this kind.
28:     fn multi_choice(&self, request: &ViewPromptParams) -> Result<Option<Vec<String>>>;
29: }
```

```text
src/server.rs:828-830
828:     if !caps.apply_edit {
829:         let _ = shared.sender.send(show_message(2, format!("magit-lsp: cannot render {} because the client does not support workspace/applyEdit", path.display())));
830:         return;

src/presentation/plain.rs:129-145
129:     /// Retain the exact structured snapshot associated with an authoritative render.
130:     pub fn render_actions(&mut self, tree: crate::tree::Tree, view_id: String, generation: u64, scope: String, menu: Option<crate::menu::Menu>) -> Option<ApplyEdit> {
131:         let text = self.format.serialize(&tree);
132:         let hash = hash_text(&text);
133:         self.action_snapshots.retain(|snapshot| snapshot.hash != hash);
134:         self.action_snapshots.push_back(ActionSnapshot { hash, view_id, generation, scope, tree, menu });
135:         while self.action_snapshots.len() > self.snapshot_limit { self.action_snapshots.pop_front(); }
136:         self.render(text)
137:     }
138:
139:     /// Find a retained action snapshot only when its exact document text matches.
140:     pub fn action_snapshot(&self, text: &str) -> Option<(&crate::tree::Tree, &str, u64, &str, Option<&crate::menu::Menu>)> {
141:         if self.format_suspended { return None; }
142:         let hash = hash_text(text);
143:         self.action_snapshots.iter().find(|snapshot| snapshot.hash == hash)
144:             .map(|snapshot| (&snapshot.tree, snapshot.view_id.as_str(), snapshot.generation, snapshot.scope.as_str(), snapshot.menu.as_ref()))
145:     }

src/presentation/plain.rs:163-183
163:     /// Observe a didChange notification, reconciling an exact applyEdit echo.
164:     pub fn change(&mut self, version: i32, text: impl Into<String>) -> Option<ApplyEdit> {
165:         if self.state == DocumentState::Detached { return None; }
166:         let text = text.into();
167:         let observed = self.snapshot_for(text, version);
168:         self.client = Some(observed.clone());
169:         self.rejected = None;
170:         if let Some(flight) = self.in_flight.as_mut() {
171:             flight.echoed = flight.target.hash == observed.hash;
172:             flight.format_drift = !flight.echoed && format_equivalent(&flight.target.text, &observed.text);
173:             flight.diverged |= !flight.echoed;
174:             if flight.response == Some(true) {
175:                 if flight.format_drift { self.suspend_for_format_drift(); return None; }
176:                 self.in_flight = None;
177:                 self.state = DocumentState::Ready;
178:             }
179:             return self.prepare();
180:         }
181:         self.state = DocumentState::Ready;
182:         self.prepare()
183:     }
```


### Complete source: src/protocol.rs

`src/protocol.rs:1-517` (verbatim, source line numbers):

```text
1: use crate::keymap::{Binding, EditorMode};
2: use crate::menu::Menu;
3: use crate::view::{Placement, ViewDescriptor, ViewKind, ViewMode, Viewport};
4: use serde::{Deserialize, Serialize};
5: use serde_json::Value;
6:
7: /// Severity of a message effect.
8: #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
9: #[serde(rename_all = "camelCase")]
10: pub enum MessageLevel {
11:     Info,
12:     Warn,
13:     Error,
14: }
15:
16: /// A user-facing consequence of a request.
17: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
18: #[serde(tag = "type", rename_all = "camelCase")]
19: pub enum Effect {
20:     /// Show a notification; `detail` and `processLogId` are optional extras.
21:     Message {
22:         level: MessageLevel,
23:         text: String,
24:         #[serde(skip_serializing_if = "Option::is_none")]
25:         hint: Option<String>,
26:         #[serde(skip_serializing_if = "Option::is_none")]
27:         detail: Option<String>,
28:         #[serde(rename = "processLogId", skip_serializing_if = "Option::is_none")]
29:         /// keep-for: TASKS §3.5/IMM-3 recorded optional process-log link; current constructors leave it None.
30:         process_log_id: Option<u64>,
31:     },
32:     /// The key is not bound here; perform its normal editor behaviour.
33:     PassThrough { key: String },
34:     /// Put `text` on the system clipboard.
35:     Clipboard { text: String },
36:     /// Create a window/buffer/panel and show it.
37:     /// keep-for: TASKS §3.5 recorded effect contract; shared server handling also supports PushView.
38:     OpenView { descriptor: ViewDescriptor },
39:     /// As `OpenView`, remembering the previous view so `PopView` returns.
40:     PushView { descriptor: ViewDescriptor },
41:     /// Close that view and focus the previous one.
42:     PopView {
43:         #[serde(rename = "viewId")]
44:         view_id: String,
45:     },
46:     /// Focus an existing view.
47:     FocusView {
48:         #[serde(rename = "viewId")]
49:         view_id: String,
50:     },
51:     /// Close every view of the session.
52:     Quit,
53:     /// Open a worktree file at a location the server resolved from its tree.
54:     /// `uri` is a `file:` URI; `line` and `column` are 0-based.
55:     VisitFile {
56:         uri: String,
57:         line: u32,
58:         column: u32,
59:     },
60:     /// Open Git's real commit-message file, or re-present the same pending run
61:     /// after failed native close. New runs start at version 1; recovery carries
62:     /// the accepted snapshot and must not replace retained client edits/versions.
63:     EditDocument {
64:         #[serde(rename = "runId")]
65:         run_id: String,
66:         uri: String,
67:         text: String,
68:         version: i32,
69:         kind: String,
70:         title: String,
71:         mode: EditDocumentMode,
72:         scope: String,
73:     },
74: }
75:
76: /// How an editor-bridge document is edited.
77: #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
78: #[serde(rename_all = "camelCase")]
79: pub enum EditDocumentMode {
80:     /// Native Git COMMIT_EDITMSG file, synchronized using standard LSP.
81:     RealFile,
82: }
83:
84: /// Run-aware semantic confirmation routed through `magit.command`.
85: #[derive(Deserialize)]
86: #[serde(rename_all = "camelCase", deny_unknown_fields)]
87: pub struct EditConfirmParams {
88:     /// Semantic command; only common.confirm is accepted by this dispatcher.
89:     pub command: String,
90:     /// Server-issued run, protecting a reused COMMIT_EDITMSG URI.
91:     pub run_id: String,
92:     /// Exact server-issued canonical file URI.
93:     pub uri: String,
94:     /// Exact synchronized version, or the close event's last observed version.
95:     pub version: i32,
96:     /// Select synchronized writeback versus independently read saved disk text.
97:     pub source: EditConfirmSource,
98: }
99:
100: /// The two intentionally distinct commit confirmation sources.
101: #[derive(Deserialize)]
102: #[serde(rename_all = "kebab-case")]
103: pub enum EditConfirmSource {
104:     /// Validate the exact live version, then safely write it before completion.
105:     Synchronized,
106:     /// Read saved disk bytes; never write an unsaved full-sync snapshot.
107:     SavedFile,
108: }
109:
110: /// Internal confirmation source after the server has validated dispatch input.
111: #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
112: #[serde(rename_all = "kebab-case")]
113: pub enum DraftFinishReason {
114:     /// Explicit confirm names the exact synchronized live version for writeback.
115:     Confirm,
116:     /// Normal close reads saved disk bytes independently of live text/version.
117:     NormalClose,
118: }
119:
120: /// Validated internal confirmation identity and source; not a wire method.
121: pub struct DraftFinishParams {
122:     /// Server-issued run identity.
123:     pub run_id: String,
124:     /// Exact opaque document URI.
125:     pub uri: String,
126:     /// Selected saved version, not necessarily the newest live version on close.
127:     pub version: i32,
128:     /// Explicit confirm or positively classified user close.
129:     pub reason: DraftFinishReason,
130: }
131:
132: /// Semantic executeCommand confirmation, returned after writeback/helper release.
133: #[derive(Serialize)]
134: #[serde(tag = "type", rename_all_fields = "camelCase")]
135: pub enum EditConfirmation {
136:     /// The selected content source was accepted and the helper released, not
137:     /// necessarily committed: Git still owns hooks and cleanup.
138:     #[serde(rename = "confirmed")]
139:     Confirmed { run_id: String, uri: String, version: i32 },
140: }
141:
142: impl Effect {
143:     /// Create a plain message effect with no hint or detail.
144:     pub fn message(level: MessageLevel, text: impl Into<String>) -> Self {
145:         Effect::Message {
146:             level,
147:             text: text.into(),
148:             hint: None,
149:             detail: None,
150:             process_log_id: None,
151:         }
152:     }
153: }
154:
155: /// A cursor position in the rendered view, 0-based.
156: #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
157: pub struct Cursor {
158:     pub line: usize,
159:     pub column: usize,
160: }
161:
162: /// An inclusive document-line selection, as produced by visual mode.
163: #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
164: #[serde(rename_all = "camelCase")]
165: pub struct Selection {
166:     pub start_line: usize,
167:     pub end_line: usize,
168: }
169:
170: /// Replace `start_line..end_line` with `lines` (end exclusive).
171: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
172: #[serde(rename_all = "camelCase")]
173: pub struct LineEdit {
174:     pub start_line: usize,
175:     pub end_line: usize,
176:     pub lines: Vec<String>,
177: }
178:
179: /// `view/render`: a full document or a line-range update.
180: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
181: #[serde(rename_all = "camelCase")]
182: pub struct ViewRender {
183:     pub view_id: String,
184:     pub version: u64,
185:     #[serde(skip_serializing_if = "Option::is_none")]
186:     pub base_version: Option<u64>,
187:     #[serde(skip_serializing_if = "Option::is_none")]
188:     pub text: Option<Vec<String>>,
189:     #[serde(skip_serializing_if = "Option::is_none")]
190:     pub edits: Option<Vec<LineEdit>>,
191:     #[serde(skip_serializing_if = "Option::is_none")]
192:     pub cursor: Option<Cursor>,
193:     /// The view mode this render belongs to, so the client can show it.
194:     pub mode: ViewMode,
195:     /// The menu steps typed so far, e.g. `-` or `g`; empty otherwise.
196:     #[serde(default, skip_serializing_if = "String::is_empty")]
197:     pub pending: String,
198:     /// The structured menu model, for menu views only (§3.11).
199:     #[serde(skip_serializing_if = "Option::is_none")]
200:     pub menu: Option<Menu>,
201: }
202:
203: /// Method name of the server's render notification.
204: pub const VIEW_RENDER: &str = "view/render";
205: /// Method name of the server's asynchronous effect notification.
206: pub const VIEW_EFFECTS: &str = "view/effects";
207:
208: /// `view/effects`: effects produced after a request already answered, such as
209: /// an `editDocument` from a detached Git process.
210: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
211: #[serde(rename_all = "camelCase")]
212: pub struct ViewEffectsParams {
213:     pub effects: Vec<Effect>,
214: }
215: /// Method name of the client's ack notification.
216: pub const VIEW_RENDERED: &str = "view/rendered";
217: /// Method name of the client's viewport notification.
218: pub const VIEW_RESIZE: &str = "view/resize";
219:
220: /// `view/open`: create a view and receive its descriptor.
221: pub enum ViewOpenRequest {}
222: #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
223: #[serde(rename_all = "camelCase")]
224: pub struct ViewOpenParams {
225:     pub kind: ViewKind,
226:     /// Kind-specific arguments. A status view carries `repo_path` here.
227:     #[serde(default, skip_serializing_if = "Option::is_none")]
228:     pub args: Option<Value>,
229:     #[serde(default, skip_serializing_if = "Option::is_none")]
230:     pub placement: Option<Placement>,
231:     pub viewport: Viewport,
232: }
233: impl ViewOpenParams {
234:     /// Repository root carried by a status view's arguments.
235:     pub fn repo_path(&self) -> Option<&str> {
236:         self.args
237:             .as_ref()?
238:             .get("repo_path")
239:             .and_then(Value::as_str)
240:     }
241: }
242: impl lsp_types::request::Request for ViewOpenRequest {
243:     type Params = ViewOpenParams;
244:     type Result = ViewDescriptor;
245:     const METHOD: &'static str = "view/open";
246: }
247:
248: /// `view/close`: drop a view from its session.
249: pub enum ViewCloseRequest {}
250: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
251: #[serde(rename_all = "camelCase")]
252: pub struct ViewCloseParams {
253:     pub view_id: String,
254: }
255: impl lsp_types::request::Request for ViewCloseRequest {
256:     type Params = ViewCloseParams;
257:     type Result = Value;
258:     const METHOD: &'static str = "view/close";
259: }
260:
261: /// `view/invoke`: run a command against a view.
262: pub enum ViewInvokeRequest {}
263: #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
264: #[serde(rename_all = "camelCase")]
265: pub struct ViewInvokeParams {
266:     pub view_id: String,
267:     pub command: String,
268:     /// Displayed render version; required for commands consuming cursor/selection.
269:     /// Only the current or last acknowledged tree is retained. Other versions
270:     /// (including an omitted version) produce a readable stale-view refusal.
271:     /// Commit selection (`common.confirm` in a selection view and
272:     /// `select.toggle-mark`) requires the exact acknowledged version.
273:     /// Non-positional commands ignore this field.
274:     #[serde(default, skip_serializing_if = "Option::is_none")]
275:     pub version: Option<u64>,
276:     #[serde(default, skip_serializing_if = "Option::is_none")]
277:     pub args: Option<Value>,
278:     #[serde(default, skip_serializing_if = "Option::is_none")]
279:     pub cursor: Option<Cursor>,
280:     #[serde(default, skip_serializing_if = "Option::is_none")]
281:     pub selection: Option<Selection>,
282: }
283: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
284: pub struct ViewInvokeResponse {
285:     pub effects: Vec<Effect>,
286: }
287: impl lsp_types::request::Request for ViewInvokeRequest {
288:     type Params = ViewInvokeParams;
289:     type Result = ViewInvokeResponse;
290:     const METHOD: &'static str = "view/invoke";
291: }
292:
293: /// `view/key`: press one canonical key step in a view.
294: pub enum ViewKeyRequest {}
295: #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
296: #[serde(rename_all = "camelCase")]
297: pub struct ViewKeyParams {
298:     pub view_id: String,
299:     /// Displayed render version for positional commands, with the same retained
300:     /// tree policy as `view/invoke`. Non-positional/menu keys ignore it.
301:     pub version: u64,
302:     /// One canonical key step, e.g. `s`, `g`, `ctrl+c` (§3.8).
303:     pub key: String,
304:     pub editor_mode: EditorMode,
305:     pub cursor: Cursor,
306:     #[serde(default, skip_serializing_if = "Option::is_none")]
307:     pub selection: Option<Selection>,
308: }
309: impl lsp_types::request::Request for ViewKeyRequest {
310:     type Params = ViewKeyParams;
311:     type Result = ViewInvokeResponse;
312:     const METHOD: &'static str = "view/key";
313: }
314:
315: /// How a client wants to receive keymaps (§3.9).
316: #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
317: pub enum KeymapDelivery {
318:     /// The server pushes `view/keymap` and the client applies it atomically.
319:     #[default]
320:     Dynamic,
321:     /// The client ships static bindings; no push is needed.
322:     Static,
323:     /// The client forwards every key; the server answers `PassThrough`.
324:     ForwardAll,
325: }
326:
327: impl KeymapDelivery {
328:     /// Parse the `capabilities.keymapDelivery` value, defaulting to `dynamic`.
329:     pub fn parse(value: &str) -> Self {
330:         match value {
331:             "static" => KeymapDelivery::Static,
332:             "forwardAll" => KeymapDelivery::ForwardAll,
333:             _ => KeymapDelivery::Dynamic,
334:         }
335:     }
336: }
337:
338: /// How faithfully a client can report key modifiers (§3.8).
339: #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
340: pub enum KeyModifiers {
341:     #[default]
342:     Full,
343:     Basic,
344: }
345:
346: impl KeyModifiers {
347:     /// Parse the `capabilities.keyModifiers` value, defaulting to `full`.
348:     pub fn parse(value: &str) -> Self {
349:         match value {
350:             "basic" => KeyModifiers::Basic,
351:             _ => KeyModifiers::Full,
352:         }
353:     }
354: }
355:
356: /// Method name of the server's keymap notification.
357: pub const VIEW_KEYMAP: &str = "view/keymap";
358:
359: /// `view/keymap`: the complete binding set for one view scope.
360: #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
361: #[serde(rename_all = "camelCase")]
362: pub struct ViewKeymapParams {
363:     pub view_id: String,
364:     pub scope: String,
365:     pub version: u64,
366:     pub capture: bool,
367:     pub bindings: Vec<Binding>,
368:     /// The view mode these bindings belong to (`text` has only the toggle).
369:     pub mode: ViewMode,
370:     /// The menu steps typed so far, e.g. `-`; empty for other views.
371:     #[serde(default, skip_serializing_if = "String::is_empty")]
372:     pub pending: String,
373:     /// The keys that do something right now, so clients need no chord logic.
374:     #[serde(default, skip_serializing_if = "Vec::is_empty")]
375:     pub live_keys: Vec<String>,
376: }
377:
378: /// `view/setMode`: switch a view between interactive and text mode.
379: pub enum ViewSetModeRequest {}
380: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
381: #[serde(rename_all = "camelCase")]
382: pub struct ViewSetModeParams {
383:     pub view_id: String,
384:     pub mode: ViewMode,
385: }
386: impl lsp_types::request::Request for ViewSetModeRequest {
387:     type Params = ViewSetModeParams;
388:     type Result = ViewInvokeResponse;
389:     const METHOD: &'static str = "view/setMode";
390: }
391:
392: /// `view/resync`: return the current full text after a lost update.
393: pub enum ViewResyncRequest {}
394: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
395: #[serde(rename_all = "camelCase")]
396: pub struct ViewResyncParams {
397:     pub view_id: String,
398: }
399: impl lsp_types::request::Request for ViewResyncRequest {
400:     type Params = ViewResyncParams;
401:     type Result = ViewRender;
402:     const METHOD: &'static str = "view/resync";
403: }
404:
405: /// `view/rendered`: the client applied a version.
406: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
407: #[serde(rename_all = "camelCase")]
408: pub struct ViewRenderedParams {
409:     pub view_id: String,
410:     pub version: u64,
411: }
412:
413: /// `view/resize`: the client's text area changed.
414: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
415: #[serde(rename_all = "camelCase")]
416: pub struct ViewResizeParams {
417:     pub view_id: String,
418:     pub cols: u32,
419:     pub rows: u32,
420: }
421:
422: /// Method name of the server's prompt notification.
423: pub const VIEW_PROMPT: &str = "view/prompt";
424: /// Method name of the client's prompt answer request.
425: pub const VIEW_PROMPT_RESPOND: &str = "view/promptRespond";
426:
427: /// The four kinds of question the server can ask (§3.6).
428: #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
429: #[serde(rename_all = "camelCase")]
430: pub enum PromptKind {
431:     Confirm,
432:     Text,
433:     Choice,
434:     MultiChoice,
435: }
436:
437: /// One selectable item of a `choice` or `multiChoice` prompt.
438: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
439: #[serde(rename_all = "camelCase")]
440: pub struct PromptItem {
441:     /// The id the server receives back; the client displays `label`.
442:     pub id: String,
443:     pub label: String,
444:     #[serde(default, skip_serializing_if = "Option::is_none")]
445:     pub description: Option<String>,
446:     #[serde(default, skip_serializing_if = "Option::is_none")]
447:     pub detail: Option<String>,
448: }
449:
450: /// `view/prompt`: one question the server is waiting on.
451: #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
452: #[serde(rename_all = "camelCase")]
453: pub struct ViewPromptParams {
454:     pub prompt_id: String,
455:     pub kind: PromptKind,
456:     /// The question, shown prominently.
457:     pub title: String,
458:     #[serde(default, skip_serializing_if = "Option::is_none")]
459:     pub detail: Option<String>,
460:     #[serde(default, skip_serializing_if = "Option::is_none")]
461:     pub yes_label: Option<String>,
462:     #[serde(default, skip_serializing_if = "Option::is_none")]
463:     pub no_label: Option<String>,
464:     #[serde(default, skip_serializing_if = "Option::is_none")]
465:     pub destructive: Option<bool>,
466:     /// Kind-dependent default: a bool, a string or an item id.
467:     #[serde(default, skip_serializing_if = "Option::is_none")]
468:     pub default: Option<Value>,
469:     #[serde(default, skip_serializing_if = "Option::is_none")]
470:     pub placeholder: Option<String>,
471:     #[serde(default, skip_serializing_if = "Option::is_none")]
472:     pub secret: Option<bool>,
473:     #[serde(default, skip_serializing_if = "Option::is_none")]
474:     pub items: Option<Vec<PromptItem>>,
475:     #[serde(default, rename = "allowCustom", skip_serializing_if = "Option::is_none")]
476:     pub allow_custom: Option<bool>,
477:     #[serde(default, skip_serializing_if = "Option::is_none")]
478:     pub preselected: Option<Vec<String>>,
479:     #[serde(default, skip_serializing_if = "Option::is_none")]
480:     pub min: Option<usize>,
481:     #[serde(default, skip_serializing_if = "Option::is_none")]
482:     pub max: Option<usize>,
483: }
484:
485: /// A `choice` answer: one item id, or the text the user typed instead.
486: #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
487: #[serde(untagged)]
488: pub enum PromptChoiceAnswer {
489:     Id { id: String },
490:     Custom { custom: String },
491: }
492:
493: /// The value of a prompt answer; its shape follows the prompt kind.
494: #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
495: #[serde(untagged)]
496: pub enum PromptValue {
497:     Bool(bool),
498:     Text(String),
499:     Choice(PromptChoiceAnswer),
500:     Multi { ids: Vec<String> },
501: }
502:
503: /// `view/promptRespond`: the client's answer to a prompt notification.
504: pub enum ViewPromptRespondRequest {}
505: #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
506: #[serde(rename_all = "camelCase")]
507: pub struct ViewPromptRespondParams {
508:     pub prompt_id: String,
509:     pub cancelled: bool,
510:     #[serde(default, skip_serializing_if = "Option::is_none")]
511:     pub value: Option<PromptValue>,
512: }
513: impl lsp_types::request::Request for ViewPromptRespondRequest {
514:     type Params = ViewPromptRespondParams;
515:     type Result = Value;
516:     const METHOD: &'static str = VIEW_PROMPT_RESPOND;
517: }
```

### Complete source: src/presentation/mod.rs

`src/presentation/mod.rs:1-313` (verbatim, source line numbers):

```text
1: //! Validated client presentation capabilities read at the LSP initialize boundary.
2:
3: use serde_json::Value;
4: use std::collections::BTreeSet;
5: use std::fmt;
6:
7: mod custom;
8: pub mod plain;
9: pub mod resolve;
10: pub use custom::{CustomRender, Ui};
11:
12: /// Presentation selected by `capabilities.experimental.magit` during initialize.
13: #[derive(Debug, Clone, PartialEq, Eq)]
14: pub enum Presentation {
15:     /// No custom selector was supplied; use the future standard-LSP presentation.
16:     Plain(PlainCapabilities),
17:     /// The client explicitly requested the versioned custom protocol.
18:     Rich(RichCapabilities),
19: }
20:
21: /// A validation error in the experimental rich presentation declaration.
22: #[derive(Debug, Clone, PartialEq, Eq)]
23: pub struct CapabilityError(pub String);
24:
25: impl fmt::Display for CapabilityError {
26:     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
27:         f.write_str(&self.0)
28:     }
29: }
30:
31: impl std::error::Error for CapabilityError {}
32:
33: /// Parse the initialize params, selecting Plain when the selector is absent.
34: /// A present `experimental.magit` declaration is strictly validated as rich v1.
35: pub fn negotiate(params: &Value) -> Result<Presentation, CapabilityError> {
36:     let caps = params.get("capabilities").unwrap_or(&Value::Null);
37:     let Some(block) = caps.pointer("/experimental/magit") else {
38:         return Ok(Presentation::Plain(PlainCapabilities::from_lsp(caps)));
39:     };
40:     let object = block
41:         .as_object()
42:         .ok_or_else(|| invalid("must be an object"))?;
43:     let version = object.get("v").and_then(Value::as_u64);
44:     if version != Some(1) {
45:         return Err(invalid("v must be 1"));
46:     }
47:     let keys = parse_enum(object, "keys", &["dynamic", "static", "forwardAll"])?;
48:     let modifiers = parse_enum(object, "modifiers", &["full", "basic"])?;
49:     let render = parse_enum(object, "render", &["edits", "full"])?;
50:     let folds = parse_enum(object, "folds", &["server", "native"])?;
51:     let layout = parse_set(object, "layout", &["main", "side", "bottom", "float"])?;
52:     if !layout.contains("main") {
53:         return Err(invalid(
54:             "layout must include main (the view stack uses main)",
55:         ));
56:     }
57:     let effects = parse_set(
58:         object,
59:         "effects",
60:         &["visitFile", "editDocument", "clipboard"],
61:     )?;
62:     let prompts = parse_set(
63:         object,
64:         "prompts",
65:         &["confirm", "text", "choice", "multiChoice", "secret"],
66:     )?;
67:     Ok(Presentation::Rich(RichCapabilities {
68:         keys,
69:         modifiers,
70:         render,
71:         layout,
72:         effects,
73:         prompts,
74:         folds,
75:     }))
76: }
77:
78: fn invalid(message: &str) -> CapabilityError {
79:     CapabilityError(format!("capabilities.experimental.magit {message}"))
80: }
81:
82: fn parse_enum(
83:     object: &serde_json::Map<String, Value>,
84:     field: &str,
85:     allowed: &[&str],
86: ) -> Result<String, CapabilityError> {
87:     let value = object
88:         .get(field)
89:         .and_then(Value::as_str)
90:         .ok_or_else(|| invalid(&format!("{field} is required and must be a string")))?;
91:     if !allowed.contains(&value) {
92:         return Err(invalid(&format!("{field} has unsupported value {value:?}")));
93:     }
94:     Ok(value.to_owned())
95: }
96:
97: fn parse_set(
98:     object: &serde_json::Map<String, Value>,
99:     field: &str,
100:     allowed: &[&str],
101: ) -> Result<BTreeSet<String>, CapabilityError> {
102:     let values = object
103:         .get(field)
104:         .and_then(Value::as_array)
105:         .ok_or_else(|| invalid(&format!("{field} is required and must be an array")))?;
106:     let mut result = BTreeSet::new();
107:     for value in values {
108:         let item = value
109:             .as_str()
110:             .ok_or_else(|| invalid(&format!("{field} entries must be strings")))?;
111:         if !allowed.contains(&item) {
112:             return Err(invalid(&format!("{field} has unsupported value {item:?}")));
113:         }
114:         if !result.insert(item.to_owned()) {
115:             return Err(invalid(&format!("{field} contains duplicate {item:?}")));
116:         }
117:     }
118:     Ok(result)
119: }
120:
121: /// Validated custom-render declarations. Core effects are mandatory protocol behavior;
122: /// `effects` and `prompts` contain only explicitly declared optional features.
123: #[derive(Debug, Clone, PartialEq, Eq)]
124: pub struct RichCapabilities {
125:     /// Key delivery strategy.
126:     pub keys: String,
127:     /// Modifier fidelity requested by the client.
128:     pub modifiers: String,
129:     /// Render transport selected by the client.
130:     pub render: String,
131:     /// Declared logical view placements; always includes `main`.
132:     pub layout: BTreeSet<String>,
133:     /// Optional effects the client can handle.
134:     pub effects: BTreeSet<String>,
135:     /// Optional prompt kinds the client can handle.
136:     pub prompts: BTreeSet<String>,
137:     /// Fold presentation strategy.
138:     pub folds: String,
139: }
140:
141: impl RichCapabilities {
142:     /// Required optional effect for a command, before any command-side work.
143:     pub fn command_requirement(&self, command: &str) -> Option<&'static str> {
144:         let required = match command {
145:             "common.visit" => "visitFile",
146:             "common.copy-value" | "log.select-and-copy" => "clipboard",
147:             "commit.create" | "commit.amend" | "commit.extend" | "commit.reword" => "editDocument",
148:             _ => return None,
149:         };
150:         (!self.supports_effect(required)).then_some(required)
151:     }
152:
153:     /// Whether both the prompt kind and any secret-text requirement are declared.
154:     pub fn allows_prompt(&self, prompt: &crate::protocol::ViewPromptParams) -> bool {
155:         use crate::protocol::PromptKind;
156:         let kind = match prompt.kind {
157:             PromptKind::Confirm => "confirm",
158:             PromptKind::Text => "text",
159:             PromptKind::Choice => "choice",
160:             PromptKind::MultiChoice => "multiChoice",
161:         };
162:         self.supports_prompt(kind)
163:             && (prompt.secret != Some(true) || self.supports_prompt("secret"))
164:     }
165:
166:     /// Fall back to the mandatory main placement without changing view identity.
167:     pub fn placement(&self, placement: crate::view::Placement) -> crate::view::Placement {
168:         use crate::view::Placement;
169:         let name = match placement {
170:             Placement::Main => "main",
171:             Placement::Side => "side",
172:             Placement::Bottom => "bottom",
173:             Placement::Float => "float",
174:         };
175:         if self.supports_placement(name) {
176:             placement
177:         } else {
178:             Placement::Main
179:         }
180:     }
181:
182:     /// Whether this declaration opts into a named optional effect.
183:     pub fn supports_effect(&self, effect: &str) -> bool {
184:         self.effects.contains(effect)
185:     }
186:     /// Whether this declaration opts into a named optional prompt kind.
187:     pub fn supports_prompt(&self, prompt: &str) -> bool {
188:         self.prompts.contains(prompt)
189:     }
190:     /// Whether a non-core placement was declared.
191:     pub fn supports_placement(&self, placement: &str) -> bool {
192:         self.layout.contains(placement)
193:     }
194: }
195:
196: /// Standard LSP capability subset relevant to the Plain presentation.
197: #[derive(Debug, Clone, Default, PartialEq, Eq)]
198: pub struct PlainCapabilities {
199:     /// Client can accept workspace edits.
200:     pub apply_edit: bool,
201:     /// Client supports versioned `WorkspaceEdit.documentChanges`.
202:     pub document_changes: bool,
203:     /// Client supports code actions.
204:     pub code_actions: bool,
205:     /// Client supports execute-command requests.
206:     pub execute_command: bool,
207:     /// Client can represent disabled code actions and reasons.
208:     pub disabled_actions: bool,
209:     /// Client supports `window/showDocument`.
210:     pub show_document: bool,
211:     /// Client supports work-done progress.
212:     pub work_done_progress: bool,
213:     /// Client supports completion.
214:     pub completion: bool,
215:     /// Client supports folding ranges.
216:     pub folding_range: bool,
217:     /// Client's advertised position encodings.
218:     pub position_encodings: Vec<String>,
219: }
220:
221: /// A standard capability with a human-readable absence reason.
222: #[derive(Debug, Clone, PartialEq, Eq)]
223: pub enum Support {
224:     /// The required standard capability is advertised.
225:     Supported,
226:     /// The capability is absent; the string explains the resulting limitation.
227:     Missing(&'static str),
228: }
229:
230: /// Availability of a Plain content action, shared by offer and execution paths.
231: #[derive(Debug, Clone, Copy, PartialEq, Eq)]
232: pub enum PlainContentAction {
233:     /// The action may be offered and dispatched.
234:     Supported,
235:     /// The client lacks required standard LSP capabilities.
236:     MissingCapabilities(&'static str),
237:     /// Discard confirmation is not yet backed by a Plain document prompt.
238:     ConfirmationUnavailable(&'static str),
239: }
240:
241: impl PlainCapabilities {
242:     /// Decide whether a Plain stage/unstage/discard action can run.
243:     pub fn content_action(&self, command: &str) -> Option<PlainContentAction> {
244:         if !matches!(command, "status.stage" | "status.unstage" | "status.discard") {
245:             return None;
246:         }
247:         if !self.apply_edit || !self.code_actions || !self.execute_command {
248:             return Some(PlainContentAction::MissingCapabilities(
249:                 "workspace.applyEdit, textDocument.codeAction, and workspace.executeCommand are required",
250:             ));
251:         }
252:         if command == "status.discard" {
253:             return Some(PlainContentAction::ConfirmationUnavailable(
254:                 "Discard is unavailable in Plain mode until document-backed confirmation is supported",
255:             ));
256:         }
257:         Some(PlainContentAction::Supported)
258:     }
259:
260:     fn from_lsp(caps: &Value) -> Self {
261:         let b = |path: &str| caps.pointer(path).and_then(Value::as_bool) == Some(true);
262:         let present = |path: &str| {
263:             caps.pointer(path)
264:                 .is_some_and(|v| !v.is_null() && v != false)
265:         };
266:         Self {
267:             apply_edit: b("/workspace/applyEdit"),
268:             document_changes: b("/workspace/workspaceEdit/documentChanges"),
269:             code_actions: present("/textDocument/codeAction"),
270:             execute_command: present("/workspace/executeCommand"),
271:             disabled_actions: b("/textDocument/codeAction/disabledSupport"),
272:             show_document: b("/window/showDocument/support"),
273:             work_done_progress: b("/window/workDoneProgress"),
274:             completion: present("/textDocument/completion"),
275:             folding_range: present("/textDocument/foldingRange"),
276:             position_encodings: caps
277:                 .pointer("/general/positionEncodings")
278:                 .and_then(Value::as_array)
279:                 .map(|xs| {
280:                     xs.iter()
281:                         .filter_map(Value::as_str)
282:                         .map(str::to_owned)
283:                         .collect()
284:                 })
285:                 .unwrap_or_default(),
286:         }
287:     }
288:
289:     /// Report whether workspace edits are available, with an explicit inert reason.
290:     pub fn edit_support(&self) -> Support {
291:         if self.apply_edit {
292:             Support::Supported
293:         } else {
294:             Support::Missing("workspace.applyEdit is not supported; presentation is inert")
295:         }
296:     }
297:     /// Report whether action invocation can be exposed.
298:     pub fn action_support(&self) -> Support {
299:         if self.code_actions && self.execute_command {
300:             Support::Supported
301:         } else {
302:             Support::Missing("textDocument.codeAction and workspace.executeCommand are required")
303:         }
304:     }
305:     /// Report whether file navigation and real-file commit editing can be exposed.
306:     pub fn show_document_support(&self) -> Support {
307:         if self.show_document {
308:             Support::Supported
309:         } else {
310:             Support::Missing("window.showDocument.support is not supported")
311:         }
312:     }
313: }
```

### Complete source: src/presentation/custom.rs

`src/presentation/custom.rs:1-241` (verbatim, source line numbers):

```text
1: //! Typed transport for the existing custom view protocol.
2:
3: use super::RichCapabilities;
4: use crate::protocol::{
5:     Effect, MessageLevel, ViewEffectsParams, ViewPromptParams, ViewRender, VIEW_EFFECTS,
6:     VIEW_PROMPT, VIEW_RENDER,
7: };
8: use lsp_server::{Message, Notification};
9: use serde::Serialize;
10: use std::sync::Arc;
11:
12: /// Client-facing typed presentation delivery seam.
13: pub trait Ui: Send + Sync {
14:     /// Build the existing wire notification for one rendered view.
15:     fn render(&self, render: ViewRender) -> Message;
16:     /// Build the existing wire notification for asynchronous effects.
17:     fn effects(&self, effects: ViewEffectsParams) -> Message;
18:     /// Build the prompt notification; the caller owns its response lifecycle.
19:     fn prompt(&self, prompt: ViewPromptParams) -> Message;
20: }
21:
22: /// Existing rich/custom wire transport, retained as the initial adapter.
23: #[derive(Clone)]
24: pub struct CustomRender {
25:     capabilities: Arc<dyn Fn() -> Option<RichCapabilities> + Send + Sync>,
26: }
27:
28: impl Default for CustomRender {
29:     fn default() -> Self {
30:         Self::new()
31:     }
32: }
33:
34: impl CustomRender {
35:     /// Construct the custom-protocol adapter.
36:     pub fn new() -> Self {
37:         Self::negotiated(|| None)
38:     }
39:
40:     /// Bind delivery to the connection's current negotiated declaration.
41:     pub fn negotiated(
42:         source: impl Fn() -> Option<RichCapabilities> + Send + Sync + 'static,
43:     ) -> Self {
44:         Self {
45:             capabilities: Arc::new(source),
46:         }
47:     }
48:
49:     /// Enforce optional effects and placement on notifications and response payloads.
50:     pub fn prepare_effects(&self, mut effects: ViewEffectsParams) -> ViewEffectsParams {
51:         if let Some(caps) = (self.capabilities)() {
52:             for effect in &mut effects.effects {
53:                 let required = match effect {
54:                     Effect::VisitFile { .. } => Some("visitFile"),
55:                     Effect::EditDocument { .. } => Some("editDocument"),
56:                     Effect::Clipboard { .. } => Some("clipboard"),
57:                     Effect::OpenView { descriptor } | Effect::PushView { descriptor } => {
58:                         descriptor.placement = caps.placement(descriptor.placement);
59:                         None
60:                     }
61:                     _ => None,
62:                 };
63:                 if let Some(required) = required.filter(|required| !caps.supports_effect(required))
64:                 {
65:                     *effect = Effect::message(
66:                         MessageLevel::Warn,
67:                         format!("Client does not support {required}"),
68:                     );
69:                 }
70:             }
71:         }
72:         effects
73:     }
74:
75:     /// Check before registering a waiter or entering an editor-helper transition.
76:     pub fn allows_prompt(&self, prompt: &ViewPromptParams) -> bool {
77:         (self.capabilities)().is_none_or(|caps| caps.allows_prompt(prompt))
78:     }
79:
80:     fn notify<T: Serialize>(method: &str, params: &T) -> Message {
81:         let value = serde_json::to_value(params).expect("protocol params serialize");
82:         Message::Notification(Notification::new(method.to_owned(), value))
83:     }
84: }
85:
86: impl Ui for CustomRender {
87:     fn render(&self, render: ViewRender) -> Message {
88:         Self::notify(VIEW_RENDER, &render)
89:     }
90:     fn effects(&self, effects: ViewEffectsParams) -> Message {
91:         Self::notify(VIEW_EFFECTS, &self.prepare_effects(effects))
92:     }
93:     fn prompt(&self, prompt: ViewPromptParams) -> Message {
94:         if !self.allows_prompt(&prompt) {
95:             return self.effects(ViewEffectsParams {
96:                 effects: vec![Effect::message(
97:                     MessageLevel::Warn,
98:                     "Client does not support this prompt",
99:                 )],
100:             });
101:         }
102:         Self::notify(VIEW_PROMPT, &prompt)
103:     }
104: }
105:
106: #[cfg(test)]
107: mod tests {
108:     use super::*;
109:     use crate::protocol::{Cursor, Effect, MessageLevel};
110:     use crate::view::ViewMode;
111:     use std::sync::Mutex;
112:
113:     #[derive(Default)]
114:     struct RecordingUi(Mutex<Vec<(&'static str, serde_json::Value)>>);
115:
116:     impl Ui for RecordingUi {
117:         fn render(&self, value: ViewRender) -> Message {
118:             self.0
119:                 .lock()
120:                 .unwrap()
121:                 .push(("render", serde_json::to_value(&value).unwrap()));
122:             Message::Notification(Notification::new(
123:                 VIEW_RENDER.to_owned(),
124:                 serde_json::to_value(value).unwrap(),
125:             ))
126:         }
127:         fn effects(&self, value: ViewEffectsParams) -> Message {
128:             self.0
129:                 .lock()
130:                 .unwrap()
131:                 .push(("effects", serde_json::to_value(&value).unwrap()));
132:             Message::Notification(Notification::new(
133:                 VIEW_EFFECTS.to_owned(),
134:                 serde_json::to_value(value).unwrap(),
135:             ))
136:         }
137:         fn prompt(&self, value: ViewPromptParams) -> Message {
138:             self.0
139:                 .lock()
140:                 .unwrap()
141:                 .push(("prompt", serde_json::to_value(&value).unwrap()));
142:             Message::Notification(Notification::new(
143:                 VIEW_PROMPT.to_owned(),
144:                 serde_json::to_value(value).unwrap(),
145:             ))
146:         }
147:     }
148:
149:     #[test]
150:     fn recording_ui_receives_typed_delivery_without_transport() {
151:         let ui = RecordingUi::default();
152:         let _ = ui.render(ViewRender {
153:             view_id: "v".into(),
154:             version: 1,
155:             base_version: None,
156:             text: Some(vec!["status".into()]),
157:             edits: None,
158:             cursor: Some(Cursor { line: 0, column: 0 }),
159:             mode: ViewMode::default(),
160:             pending: String::new(),
161:             menu: None,
162:         });
163:         let _ = ui.effects(ViewEffectsParams {
164:             effects: vec![Effect::message(MessageLevel::Info, "ok")],
165:         });
166:         let _ = ui.prompt(ViewPromptParams {
167:             prompt_id: "p1".into(),
168:             kind: crate::protocol::PromptKind::Confirm,
169:             title: "Continue?".into(),
170:             detail: None,
171:             yes_label: None,
172:             no_label: None,
173:             destructive: None,
174:             default: None,
175:             placeholder: None,
176:             secret: None,
177:             items: None,
178:             allow_custom: None,
179:             preselected: None,
180:             min: None,
181:             max: None,
182:         });
183:         let got = ui.0.lock().unwrap();
184:         assert_eq!(got.len(), 3);
185:         assert_eq!(got[0].0, "render");
186:         assert_eq!(got[1].0, "effects");
187:         assert_eq!(got[2].0, "prompt");
188:         assert_eq!(got[2].1["promptId"], "p1");
189:     }
190:
191:     #[test]
192:     fn custom_adapter_preserves_notification_wire_shape() {
193:         let ui = CustomRender::new();
194:         let Message::Notification(render) = ui.render(ViewRender {
195:             view_id: "v".into(),
196:             version: 1,
197:             base_version: None,
198:             text: Some(vec!["hello".into()]),
199:             edits: None,
200:             cursor: None,
201:             mode: ViewMode::default(),
202:             pending: String::new(),
203:             menu: None,
204:         }) else {
205:             panic!("notification expected")
206:         };
207:         assert_eq!(render.method, VIEW_RENDER);
208:         assert_eq!(render.params["viewId"], "v");
209:         assert_eq!(render.params["text"][0], "hello");
210:         let Message::Notification(note) = ui.effects(ViewEffectsParams {
211:             effects: vec![Effect::message(MessageLevel::Info, "ok")],
212:         }) else {
213:             panic!("notification expected")
214:         };
215:         assert_eq!(note.method, VIEW_EFFECTS);
216:         assert_eq!(note.params["effects"][0]["type"], "message");
217:         assert_eq!(note.params["effects"][0]["text"], "ok");
218:         let Message::Notification(prompt) = ui.prompt(ViewPromptParams {
219:             prompt_id: "p".into(),
220:             kind: crate::protocol::PromptKind::Confirm,
221:             title: "Question".into(),
222:             detail: None,
223:             yes_label: None,
224:             no_label: None,
225:             destructive: None,
226:             default: None,
227:             placeholder: None,
228:             secret: None,
229:             items: None,
230:             allow_custom: None,
231:             preselected: None,
232:             min: None,
233:             max: None,
234:         }) else {
235:             panic!("notification expected")
236:         };
237:         assert_eq!(prompt.method, VIEW_PROMPT);
238:         assert_eq!(prompt.params["promptId"], "p");
239:         assert_eq!(prompt.params["title"], "Question");
240:     }
241: }
```

