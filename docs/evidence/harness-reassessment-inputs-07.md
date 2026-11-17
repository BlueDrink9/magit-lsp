### Complete source: src/presentation/plain.rs

`src/presentation/plain.rs:1-327` (verbatim, source line numbers):

```text
1: //! Standard-LSP document presentation synchronization.
2:
3: use crate::docstore::Encoding;
4: use serde_json::{json, Value};
5: use sha2::{Digest, Sha256};
6: use std::collections::VecDeque;
7:
8: /// Lifecycle of a Plain document controller.
9: #[derive(Clone, Copy, Debug, PartialEq, Eq)]
10: pub enum DocumentState {
11:     /// No editor document is currently attached.
12:     Detached,
13:     /// The editor is synchronized and an edit may be sent.
14:     Ready,
15:     /// One workspace/applyEdit request is awaiting reconciliation.
16:     Applying,
17: }
18:
19: /// One server-initiated edit request for `workspace/applyEdit`.
20: #[derive(Clone, Debug, PartialEq)]
21: pub struct ApplyEdit {
22:     /// Monotonic controller-local identifier used to discard late responses.
23:     pub id: u64,
24:     /// Standard LSP `WorkspaceEdit` payload.
25:     pub edit: Value,
26: }
27:
28: /// Immutable line ending and EOF convention captured from the accepted didOpen text.
29: #[derive(Clone, Copy, Debug, PartialEq, Eq)]
30: pub struct DocumentFormat {
31:     /// Whether serialized rows use CRLF rather than LF separators.
32:     pub crlf: bool,
33:     /// Whether serialized text ends with the captured final line terminator.
34:     pub final_terminator: bool,
35: }
36:
37: impl DocumentFormat {
38:     /// Infer the document's newline convention without changing its accepted text.
39:     pub fn from_text(text: &str) -> Self {
40:         let crlf = text.contains("\r\n");
41:         Self { crlf, final_terminator: text.ends_with('\n') }
42:     }
43:
44:     /// Serialize structured Tree rows using this document's immutable newline convention.
45:     pub fn serialize(self, tree: &crate::tree::Tree) -> String {
46:         let separator = if self.crlf { "\r\n" } else { "\n" };
47:         let mut text = tree.lines.join(separator);
48:         if self.final_terminator { text.push_str(separator); }
49:         text
50:     }
51: }
52:
53: #[derive(Clone, Debug)]
54: struct Snapshot {
55:     hash: [u8; 32],
56:     text: String,
57:     version: i32,
58: }
59:
60: #[derive(Clone, Debug)]
61: struct ActionSnapshot {
62:     hash: [u8; 32],
63:     view_id: String,
64:     generation: u64,
65:     scope: String,
66:     tree: crate::tree::Tree,
67:     menu: Option<crate::menu::Menu>,
68: }
69:
70: #[derive(Clone, Debug)]
71: struct InFlight {
72:     request: ApplyEdit,
73:     target: Snapshot,
74:     echoed: bool,
75:     diverged: bool,
76:     response: Option<bool>,
77:     format_drift: bool,
78: }
79:
80: /// Serializes authoritative rendered text into standard workspace edits.
81: ///
82: /// The controller owns synchronization state only; callers supply document
83: /// notifications and transport responses and send returned edits themselves.
84: pub struct PlainDocument {
85:     uri: String,
86:     encoding: Encoding,
87:     format: DocumentFormat,
88:     document_changes: bool,
89:     state: DocumentState,
90:     authoritative: Option<Snapshot>,
91:     client: Option<Snapshot>,
92:     snapshots: VecDeque<Snapshot>,
93:     action_snapshots: VecDeque<ActionSnapshot>,
94:     snapshot_limit: usize,
95:     in_flight: Option<InFlight>,
96:     /// A rejected target is not retried until the client or render advances.
97:     rejected: Option<([u8; 32], [u8; 32])>,
98:     next_id: u64,
99:     format_suspended: bool,
100:     warning_pending: bool,
101: }
102:
103: impl PlainDocument {
104:     /// Create a detached controller for one document URI.
105:     pub fn new(uri: impl Into<String>, encoding: Encoding, document_changes: bool, format: DocumentFormat) -> Self {
106:         Self {
107:             uri: uri.into(), encoding, document_changes, format,
108:             state: DocumentState::Detached, authoritative: None, client: None,
109:             snapshots: VecDeque::new(), snapshot_limit: 6, in_flight: None, rejected: None, next_id: 0,
110:             action_snapshots: VecDeque::new(), format_suspended: false, warning_pending: false,
111:         }
112:     }
113:
114:     /// Return the current lifecycle state.
115:     pub fn state(&self) -> DocumentState { self.state }
116:
117:     /// Return a snapshot text when its exact-text hash is retained.
118:     pub fn snapshot(&self, hash: &[u8; 32]) -> Option<(&str, i32)> {
119:         self.snapshots.iter().find(|s| &s.hash == hash).map(|s| (s.text.as_str(), s.version))
120:     }
121:
122:     /// Record an authoritative render and start an edit if the client is ready.
123:     pub fn render(&mut self, text: impl Into<String>) -> Option<ApplyEdit> {
124:         self.authoritative = Some(self.snapshot_for(text.into(), 0));
125:         self.rejected = None;
126:         self.prepare()
127:     }
128:
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
146:
147:     /// Consume the one user-facing warning caused by persistent format drift.
148:     pub fn take_format_warning(&mut self) -> bool {
149:         std::mem::replace(&mut self.warning_pending, false)
150:     }
151:
152:     /// Attach/reopen the document with its exact current editor snapshot.
153:     pub fn open(&mut self, version: i32, text: impl Into<String>) -> Option<ApplyEdit> {
154:         self.in_flight = None;
155:         self.format_suspended = false;
156:         self.warning_pending = false;
157:         self.client = Some(self.snapshot_for(text.into(), version));
158:         self.rejected = None;
159:         self.state = DocumentState::Ready;
160:         self.prepare()
161:     }
162:
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
184:
185:     /// Resolve an applyEdit result; late responses after close/reopen are ignored.
186:     pub fn response(&mut self, id: u64, applied: bool) -> Option<ApplyEdit> {
187:         let Some(flight) = self.in_flight.as_mut() else { return None; };
188:         if flight.request.id != id { return None; }
189:         flight.response = Some(applied);
190:         if !applied {
191:             if !flight.diverged {
192:                 self.rejected = Some((flight.target.hash, self.client.as_ref().map_or([0; 32], |client| client.hash)));
193:             }
194:             self.in_flight = None;
195:             self.state = DocumentState::Ready;
196:             return self.prepare();
197:         }
198:         if flight.format_drift { self.suspend_for_format_drift(); return None; }
199:         if flight.echoed || flight.diverged {
200:             self.in_flight = None;
201:             self.state = DocumentState::Ready;
202:             return self.prepare();
203:         }
204:         None
205:     }
206:
207:     /// Detach and invalidate pending responses so no late result can resurrect it.
208:     pub fn close(&mut self) {
209:         self.state = DocumentState::Detached;
210:         self.client = None;
211:         self.in_flight = None;
212:         self.rejected = None;
213:         self.format_suspended = false;
214:         self.warning_pending = false;
215:     }
216:
217:     fn snapshot_for(&mut self, text: String, version: i32) -> Snapshot {
218:         let hash = hash_text(&text);
219:         let snapshot = Snapshot { hash, text, version };
220:         self.snapshots.retain(|s| s.hash != hash);
221:         self.snapshots.push_back(snapshot.clone());
222:         while self.snapshots.len() > self.snapshot_limit { self.snapshots.pop_front(); }
223:         snapshot
224:     }
225:
226:     fn prepare(&mut self) -> Option<ApplyEdit> {
227:         if self.state == DocumentState::Detached || self.in_flight.is_some() || self.format_suspended { return None; }
228:         let (Some(authoritative), Some(client)) = (&self.authoritative, &self.client) else { return None; };
229:         if authoritative.hash == client.hash {
230:             self.state = DocumentState::Ready;
231:             return None;
232:         }
233:         if self.rejected == Some((authoritative.hash, client.hash)) {
234:             self.state = DocumentState::Ready;
235:             return None;
236:         }
237:         self.next_id += 1;
238:         let edits = text_edits(&client.text, &authoritative.text, self.encoding);
239:         let edit = if self.document_changes {
240:             json!({"documentChanges":[{"textDocument":{"uri":self.uri,"version":client.version},"edits":edits}]})
241:         } else {
242:             json!({"changes":{self.uri.clone():edits}})
243:         };
244:         let request = ApplyEdit { id: self.next_id, edit };
245:         let target = authoritative.clone();
246:         self.in_flight = Some(InFlight { request: request.clone(), target, echoed: false, diverged: false, response: None, format_drift: false });
247:         self.state = DocumentState::Applying;
248:         Some(request)
249:     }
250:
251:     fn suspend_for_format_drift(&mut self) {
252:         self.in_flight = None;
253:         self.state = DocumentState::Ready;
254:         self.format_suspended = true;
255:         self.warning_pending = true;
256:     }
257: }
258:
259: fn format_equivalent(expected: &str, observed: &str) -> bool {
260:     fn logical(text: &str) -> String {
261:         let normalized = text.replace("\r\n", "\n");
262:         normalized.strip_suffix('\n').unwrap_or(&normalized).to_owned()
263:     }
264:     expected != observed && logical(expected) == logical(observed)
265: }
266:
267: #[cfg(test)]
268: mod tests {
269:     use super::*;
270:     use crate::tree::Tree;
271:
272:     fn tree(text: &str) -> Tree {
273:         Tree { version: 0, lines: vec![text.to_owned()], nodes: vec![], line_nodes: vec![], tokens: vec![], symbols: vec![], links: vec![] }
274:     }
275:
276:     #[test]
277:     fn action_snapshot_retention_is_bounded_and_keeps_generation() {
278:         let mut document = PlainDocument::new("file:///test", Encoding::Utf16, false, DocumentFormat::from_text(""));
279:         for generation in 0..7 {
280:             document.render_actions(tree(&format!("render-{generation}")), "view".into(), generation, "scope".into(), None);
281:         }
282:         assert!(document.action_snapshot("render-0").is_none());
283:         let (_, _, generation, _, _) = document.action_snapshot("render-6").unwrap();
284:         assert_eq!(generation, 6);
285:     }
286: }
287:
288: /// Hash exact UTF-8 text for collision-resistant snapshot matching.
289: pub fn hash_text(text: &str) -> [u8; 32] { Sha256::digest(text.as_bytes()).into() }
290:
291: /// Produce a single minimal valid edit using negotiated LSP position units.
292: pub fn text_edits(old: &str, new: &str, encoding: Encoding) -> Vec<Value> {
293:     if old == new { return Vec::new(); }
294:     let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a,b)| a == b).count();
295:     while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) { prefix -= 1; }
296:     while splits_crlf(old, prefix) || splits_crlf(new, prefix) { prefix -= 1; }
297:     let mut suffix = 0;
298:     let max_suffix = old.len().saturating_sub(prefix).min(new.len().saturating_sub(prefix));
299:     while suffix < max_suffix && old.as_bytes()[old.len()-1-suffix] == new.as_bytes()[new.len()-1-suffix] { suffix += 1; }
300:     while !old.is_char_boundary(old.len()-suffix) || !new.is_char_boundary(new.len()-suffix) { suffix -= 1; }
301:     while splits_crlf(old, old.len()-suffix) || splits_crlf(new, new.len()-suffix) { suffix -= 1; }
302:     let old_end = old.len() - suffix;
303:     let new_end = new.len() - suffix;
304:     let start = position(old, prefix, encoding);
305:     let end = position(old, old_end, encoding);
306:     vec![json!({"range":{"start":start,"end":end},"newText":&new[prefix..new_end]})]
307: }
308:
309: fn splits_crlf(text: &str, offset: usize) -> bool {
310:     offset > 0 && offset < text.len()
311:         && text.as_bytes()[offset - 1] == b'\r'
312:         && text.as_bytes()[offset] == b'\n'
313: }
314:
315: fn position(text: &str, offset: usize, encoding: Encoding) -> Value {
316:     let before = &text[..offset];
317:     let line = before.bytes().filter(|b| *b == b'\n').count();
318:     let line_start = before.rfind('\n').map_or(0, |i| i + 1);
319:     let segment = &before[line_start..];
320:     let segment = segment.strip_suffix('\r').unwrap_or(segment);
321:     let character = match encoding {
322:         Encoding::Utf8 => segment.len(),
323:         Encoding::Utf16 => segment.encode_utf16().count(),
324:         Encoding::Utf32 => segment.chars().count(),
325:     };
326:     json!({"line":line,"character":character})
327: }
```

### Complete source: src/presentation/resolve.rs

`src/presentation/resolve.rs:1-60` (verbatim, source line numbers):

```text
1: //! Repository candidates discovered for Plain document initialization.
2:
3: use std::path::{Path, PathBuf};
4: use crate::git::GitRepo;
5:
6: /// Canonical identity and worktree paths reported by Git.
7: #[derive(Debug, Clone, PartialEq, Eq)]
8: pub struct Repository {
9:     /// Canonical worktree root, used for session state.
10:     pub root: PathBuf,
11:     /// Per-worktree Git directory (distinct for linked worktrees).
12:     pub git_dir: PathBuf,
13:     /// Shared Git directory (common to linked worktrees).
14:     pub common_dir: PathBuf,
15: }
16:
17: /// Resolution never chooses between multiple repositories implicitly.
18: #[derive(Debug, Clone, PartialEq, Eq)]
19: pub enum Resolution {
20:     /// One repository was unambiguously selected.
21:     Resolved(Repository),
22:     /// No repository applies; candidates can later be presented to the user.
23:     Choose(Vec<Repository>),
24:     /// No repository candidate was found.
25:     None,
26: }
27:
28: /// Resolve a document using explicit path, workspace roots, then document ancestry.
29: pub fn resolve(explicit: Option<&Path>, workspaces: &[PathBuf], document: Option<&Path>) -> Resolution {
30:     if let Some(path) = explicit {
31:         return GitRepo::discover(path).ok().flatten().map(repository).map_or(Resolution::None, Resolution::Resolved);
32:     }
33:     let workspace_candidates = unique(workspaces.iter().filter_map(|p| GitRepo::discover(p).ok().flatten().map(repository)));
34:     if workspace_candidates.len() == 1 {
35:         return Resolution::Resolved(workspace_candidates[0].clone());
36:     }
37:     if workspace_candidates.len() > 1 {
38:         return Resolution::Choose(workspace_candidates);
39:     }
40:     if let Some(path) = document {
41:         if let Some(repo) = GitRepo::discover(path).ok().flatten().map(repository) {
42:             return Resolution::Resolved(repo);
43:         }
44:     }
45:     Resolution::None
46: }
47:
48: fn repository(repo: GitRepo) -> Repository {
49:     Repository { root: repo.root().to_owned(), git_dir: repo.git_dir().to_owned(), common_dir: repo.common_dir().to_owned() }
50: }
51:
52: fn unique(repositories: impl Iterator<Item = Repository>) -> Vec<Repository> {
53:     let mut result = Vec::new();
54:     for repo in repositories {
55:         if !result.iter().any(|existing: &Repository| existing.root == repo.root) {
56:             result.push(repo);
57:         }
58:     }
59:     result
60: }
```

### Complete source: src/ui.rs

`src/ui.rs:1-199` (verbatim, source line numbers):

```text
1: //! The server-side handle commands use to ask the user a question (R-3.1).
2: //!
3: //! Handlers prepare typed continuations rather than waiting. The server drives
4: //! [`PromptUi`] outside session ownership; synchronous callers drive the same
5: //! start/resume path with [`ScriptedUi`]. `None` means the user cancelled.
6: //!
7: //! The prompt transport is a notification plus a separate `view/promptRespond`
8: //! request; see TASKS.md §10 for why it deliberately differs from §3.6.
9:
10: use crate::protocol::{PromptChoiceAnswer, PromptKind, PromptValue, ViewPromptParams};
11: use crate::{Error, Result};
12: use std::collections::VecDeque;
13: use std::sync::Mutex;
14:
15: /// Every question a command may ask; each returns `None` when cancelled.
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
30:
31: /// Drive one prepared question, outside session ownership in the server.
32: pub fn answer(ui: &dyn PromptUi, request: &ViewPromptParams) -> Result<Option<PromptValue>> {
33:     match request.kind {
34:         PromptKind::Confirm => Ok(ui.confirm(request)?.map(PromptValue::Bool)),
35:         PromptKind::Text => Ok(ui.text(request)?.map(PromptValue::Text)),
36:         PromptKind::Choice => Ok(ui.choice(request)?.map(PromptValue::Choice)),
37:         PromptKind::MultiChoice => Ok(ui.multi_choice(request)?.map(|ids| PromptValue::Multi { ids })),
38:     }
39: }
40:
41: /// A UI that refuses every prompt; the default for non-interactive callers.
42: #[derive(Debug, Default, Clone, Copy)]
43: pub struct NullUi;
44:
45: impl PromptUi for NullUi {
46:     fn confirm(&self, _request: &ViewPromptParams) -> Result<Option<bool>> {
47:         Err(prompts_unavailable())
48:     }
49:
50:     fn text(&self, _request: &ViewPromptParams) -> Result<Option<String>> {
51:         Err(prompts_unavailable())
52:     }
53:
54:     fn choice(&self, _request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>> {
55:         Err(prompts_unavailable())
56:     }
57:
58:     fn multi_choice(&self, _request: &ViewPromptParams) -> Result<Option<Vec<String>>> {
59:         Err(prompts_unavailable())
60:     }
61: }
62:
63: fn prompts_unavailable() -> Error {
64:     Error::new("prompts are unavailable")
65: }
66:
67: /// One queued answer; `Cancel` matches any prompt kind.
68: #[derive(Debug, Clone, PartialEq)]
69: enum ScriptedAnswer {
70:     Confirm(Option<bool>),
71:     Text(Option<String>),
72:     Choice(Option<PromptChoiceAnswer>),
73:     Multi(Option<Vec<String>>),
74:     Cancel,
75: }
76:
77: /// A UI answered from a queue, recording every request it received.
78: #[derive(Debug, Default)]
79: pub struct ScriptedUi {
80:     answers: Mutex<VecDeque<ScriptedAnswer>>,
81:     requests: Mutex<Vec<ViewPromptParams>>,
82: }
83:
84: impl ScriptedUi {
85:     /// An empty UI; script an answer before each prompt.
86:     pub fn new() -> Self {
87:         Self::default()
88:     }
89:
90:     /// Answer the next confirm prompt with `value`.
91:     pub fn reply_confirm(&self, value: bool) -> &Self {
92:         self.push(ScriptedAnswer::Confirm(Some(value)));
93:         self
94:     }
95:
96:     /// Answer the next text prompt with `value`; `None` cancels it.
97:     pub fn reply_text(&self, value: Option<String>) -> &Self {
98:         self.push(ScriptedAnswer::Text(value));
99:         self
100:     }
101:
102:     /// Answer the next choice prompt; `None` cancels it.
103:     pub fn reply_choice(&self, value: Option<PromptChoiceAnswer>) -> &Self {
104:         self.push(ScriptedAnswer::Choice(value));
105:         self
106:     }
107:
108:     /// Answer the next multi-choice prompt; `None` cancels it.
109:     pub fn reply_multi(&self, value: Option<Vec<String>>) -> &Self {
110:         self.push(ScriptedAnswer::Multi(value));
111:         self
112:     }
113:
114:     /// Cancel the next prompt of any kind.
115:     pub fn reply_cancel(&self) -> &Self {
116:         self.push(ScriptedAnswer::Cancel);
117:         self
118:     }
119:
120:     /// Every request this UI received, in the order it saw them.
121:     pub fn requests(&self) -> Vec<ViewPromptParams> {
122:         self.lock_requests().clone()
123:     }
124:
125:     /// The most recent request, if any.
126:     pub fn last_request(&self) -> Option<ViewPromptParams> {
127:         self.lock_requests().last().cloned()
128:     }
129:
130:     fn push(&self, answer: ScriptedAnswer) {
131:         let mut answers = self
132:             .answers
133:             .lock()
134:             .unwrap_or_else(|poisoned| poisoned.into_inner());
135:         answers.push_back(answer);
136:     }
137:
138:     fn record(&self, request: &ViewPromptParams) {
139:         self.lock_requests().push(request.clone());
140:     }
141:
142:     fn take(&self) -> Result<ScriptedAnswer> {
143:         self.answers
144:             .lock()
145:             .unwrap_or_else(|poisoned| poisoned.into_inner())
146:             .pop_front()
147:             .ok_or_else(|| Error::new("ScriptedUi: no scripted answer"))
148:     }
149:
150:     fn lock_requests(&self) -> std::sync::MutexGuard<'_, Vec<ViewPromptParams>> {
151:         self.requests
152:             .lock()
153:             .unwrap_or_else(|poisoned| poisoned.into_inner())
154:     }
155: }
156:
157: impl PromptUi for ScriptedUi {
158:     fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>> {
159:         self.record(request);
160:         match self.take()? {
161:             ScriptedAnswer::Confirm(value) => Ok(value),
162:             ScriptedAnswer::Cancel => Ok(None),
163:             other => Err(mismatch("confirm", &other)),
164:         }
165:     }
166:
167:     fn text(&self, request: &ViewPromptParams) -> Result<Option<String>> {
168:         self.record(request);
169:         match self.take()? {
170:             ScriptedAnswer::Text(value) => Ok(value),
171:             ScriptedAnswer::Cancel => Ok(None),
172:             other => Err(mismatch("text", &other)),
173:         }
174:     }
175:
176:     fn choice(&self, request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>> {
177:         self.record(request);
178:         match self.take()? {
179:             ScriptedAnswer::Choice(value) => Ok(value),
180:             ScriptedAnswer::Cancel => Ok(None),
181:             other => Err(mismatch("choice", &other)),
182:         }
183:     }
184:
185:     fn multi_choice(&self, request: &ViewPromptParams) -> Result<Option<Vec<String>>> {
186:         self.record(request);
187:         match self.take()? {
188:             ScriptedAnswer::Multi(value) => Ok(value),
189:             ScriptedAnswer::Cancel => Ok(None),
190:             other => Err(mismatch("multiChoice", &other)),
191:         }
192:     }
193: }
194:
195: fn mismatch(kind: &str, answer: &ScriptedAnswer) -> Error {
196:     Error::new(format!(
197:         "ScriptedUi: expected a {kind} answer, got {answer:?}"
198:     ))
199: }
```

### Dispatch protocol/action target definitions

`src/dispatch.rs:9-137` (verbatim, source line numbers):

```text
9: /// A content-addressed action target tied to an existing server node identity.
10: #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
11: pub struct ActionTarget {
12:     /// Existing path/content-derived tree node identity.
13:     pub node_id: String,
14:     /// Hash of the exact content the action is permitted to mutate.
15:     pub mutation_hash: String,
16:     /// Exact inclusive document rows selected in the offered render, when this
17:     /// action represents a visual selection rather than a cursor action.
18:     #[serde(default, skip_serializing_if = "Option::is_none")]
19:     pub selection: Option<SelectionIntent>,
20: }
21:
22: /// Normalized inclusive row coordinates retained as Plain selection input metadata.
23: #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
24: #[serde(rename_all = "camelCase")]
25: pub struct SelectionIntent {
26:     /// First selected row in the retained render.
27:     pub start_line: u32,
28:     /// Last selected row in the retained render.
29:     pub end_line: u32,
30: }
31:
32: /// A Plain `workspace/executeCommand` action offered from a retained render.
33: #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
34: #[serde(rename_all = "camelCase")]
35: pub struct PlainActionParams {
36:     /// Registered command identifier.
37:     pub command: String,
38:     /// Open document URI the action was offered in.
39:     pub document_uri: String,
40:     /// Typed target from the offering render.
41:     pub target: PlainActionTarget,
42:     /// Render generation the action was offered from.
43:     pub view_generation: u64,
44: }
45:
46: /// A content mutation target or a non-mutating logical-view target.
47: #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
48: #[serde(untagged)]
49: pub enum PlainActionTarget {
50:     /// Content-addressed target from a status tree.
51:     Content(ActionTarget),
52:     /// Identity of the exact logical view that offered a semantic transition.
53:     View(SemanticViewTarget),
54:     /// Exact node identity in the retained status view for visibility actions.
55:     Node(SemanticNodeTarget),
56:     /// Exact item identity in the retained structured menu.
57:     MenuItem(SemanticMenuItemTarget),
58: }
59:
60: /// Semantic status-node identity used by local and global visibility actions.
61: #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
62: #[serde(rename_all = "camelCase", deny_unknown_fields)]
63: pub struct SemanticNodeTarget {
64:     /// Logical view identity in the offering render.
65:     pub view_id: String,
66:     /// Existing node identity in the offering tree.
67:     pub node_id: String,
68: }
69:
70: /// Classify implemented visibility commands without side effects.
71: pub fn visibility_command(command: &str) -> Option<crate::view::VisibilityCommand> {
72:     use crate::view::VisibilityCommand as V;
73:     Some(match command {
74:         "common.toggle-section" => V::Toggle { recursive: false },
75:         "common.show-section" => V::Show { recursive: false },
76:         "common.hide-section" => V::Hide { recursive: false },
77:         "common.show-all" => V::ShowAll,
78:         "common.hide-all" => V::HideAll,
79:         "common.more-detail" => V::MoreDetail,
80:         "common.less-detail" => V::LessDetail,
81:         "common.detail-level-1" => V::SetDetail(1),
82:         "common.detail-level-2" => V::SetDetail(2),
83:         "common.detail-level-3" => V::SetDetail(3),
84:         "common.detail-level-4" => V::SetDetail(4),
85:         _ => return None,
86:     })
87: }
88:
89: /// Semantic view identity used for supported menu and Back transitions.
90: #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
91: #[serde(rename_all = "camelCase")]
92: #[serde(deny_unknown_fields)]
93: pub struct SemanticViewTarget {
94:     /// Logical view identity in the offering render.
95:     pub view_id: String,
96: }
97:
98: /// Typed identity of an item in one logical menu view.
99: #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
100: #[serde(rename_all = "camelCase")]
101: #[serde(deny_unknown_fields)]
102: pub struct SemanticMenuItemTarget {
103:     /// Logical view identity in the offering render.
104:     pub view_id: String,
105:     /// Structured menu identity in the offering render.
106:     pub menu_id: String,
107:     /// Exact retained menu item key.
108:     pub item_key: String,
109: }
110:
111: /// The displayed view location and input context bound to a dispatch request.
112: #[derive(Debug, Clone, PartialEq)]
113: pub struct DispatchTarget {
114:     /// Server view identity.
115:     pub view_id: String,
116:     /// Exact displayed render version, when positional input is used.
117:     pub version: Option<u64>,
118:     /// Cursor from that displayed render.
119:     pub cursor: Option<Cursor>,
120:     /// Inclusive selection from that displayed render.
121:     pub selection: Option<Selection>,
122:     /// Canonical key chord, when dispatched from a key.
123:     pub key: Option<String>,
124: }
125:
126: /// A semantic command request shared by invocation and resolved key paths.
127: #[derive(Debug, Clone, PartialEq)]
128: pub struct DispatchRequest {
129:     /// Registered command identifier.
130:     pub command: String,
131:     /// Display-version-bound input target.
132:     pub target: DispatchTarget,
133:     /// Optional content target offered from the named displayed render.
134:     pub action_target: Option<ActionTarget>,
135:     /// Typed command arguments, including captured menu arguments.
136:     pub args: Option<Value>,
137: }
```

### Dispatch function/type index (complete grep output)

```text
11:pub struct ActionTarget {
25:pub struct SelectionIntent {
35:pub struct PlainActionParams {
49:pub enum PlainActionTarget {
63:pub struct SemanticNodeTarget {
71:pub fn visibility_command(command: &str) -> Option<crate::view::VisibilityCommand> {
93:pub struct SemanticViewTarget {
102:pub struct SemanticMenuItemTarget {
113:pub struct DispatchTarget {
128:pub struct DispatchRequest {
140:pub fn content_command(command: &str) -> bool {
145:pub fn hunk_section_compatible(command: &str, section: Option<&str>) -> bool {
155:fn file_identity(tree: &crate::tree::Tree, node: &crate::tree::Node) -> Option<crate::git::FileMutationIdentity> {
165:fn mutation_hash(
189:pub fn offer_target(tree: &crate::tree::Tree, command: &str, node_id: &str) -> Option<ActionTarget> {
220:pub fn offer_selection_target(tree: &crate::tree::Tree, command: &str, start: u32, end: u32) -> Option<ActionTarget> {
235:fn selection_members(tree: &crate::tree::Tree, command: &str, start: u32, end: u32, section: &str) -> Option<Vec<(String, String)>> {
253:pub fn target_is_current(repo: &GitRepo, tree: &crate::tree::Tree, command: &str, target: &ActionTarget) -> Result<bool> {
290:pub fn displayed_targets(tree: &crate::tree::Tree, command: &str, cursor: Option<Cursor>, selection: Option<Selection>) -> Vec<ActionTarget> {
317:pub enum Command {
328:pub enum NodeKind {
349:pub struct LineRange {
356:pub enum Resolution {
398:pub const NOTHING_TO_STAGE: &str = "Nothing to stage here";
400:pub const NOTHING_TO_UNSTAGE: &str = "Nothing to unstage here";
402:pub const NOTHING_TO_DISCARD: &str = "Nothing to discard here";
404:fn info(text: &str) -> Resolution {
410:pub fn resolve(command: Command, target: NodeKind, selection: Option<LineRange>) -> Resolution {
468:pub fn execute(
```

### Dispatch target production and validation

`src/dispatch.rs:189-315` (verbatim, source line numbers):

```text
189: pub fn offer_target(tree: &crate::tree::Tree, command: &str, node_id: &str) -> Option<ActionTarget> {
190:     use crate::tree::{MutationContent, NodeKind};
191:     if !content_command(command) { return None; }
192:     let mut node = tree.node(node_id)?;
193:     if node.kind == NodeKind::Section {
194:         let section = node.section.as_deref()?;
195:         let applicable = match command {
196:             "status.stage" => matches!(section, "unstaged" | "untracked"),
197:             "status.unstage" => section == "staged",
198:             _ => false,
199:         };
200:         if !applicable { return None; }
201:         use sha2::{Digest, Sha256};
202:         let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&(command, section)).ok()?));
203:         return Some(ActionTarget { node_id: node.id.clone(), mutation_hash: hash, selection: None });
204:     }
205:     if node.kind == NodeKind::Line {
206:         node = tree.nodes.iter().find(|hunk| hunk.kind == NodeKind::Hunk
207:             && hunk.path == node.path && hunk.section == node.section && hunk.hunk_id == node.hunk_id)?;
208:     }
209:     if matches!(node.kind, NodeKind::Hunk | NodeKind::Line)
210:         && !hunk_section_compatible(command, node.section.as_deref()) { return None; }
211:     let identity = file_identity(tree, node)?;
212:     let hunk = match node.mutation.as_ref()? {
213:         MutationContent::File(_) => None,
214:         MutationContent::Hunk { hunk, identical } => Some((hunk, *identical)),
215:     };
216:     Some(ActionTarget { node_id: node.id.clone(), mutation_hash: mutation_hash(command, identity, hunk), selection: None })
217: }
218:
219: /// Offer a content-addressed target for an exact inclusive selection in a render.
220: pub fn offer_selection_target(tree: &crate::tree::Tree, command: &str, start: u32, end: u32) -> Option<ActionTarget> {
221:     if !content_command(command) { return None; }
222:     let (start, end) = (start.min(end), start.max(end));
223:     let section = if command == "status.unstage" { "staged" } else { "unstaged" };
224:     if tree.line_nodes.get(start as usize).and_then(|line| line.section.as_deref()) != Some(section) { return None; }
225:     let members = selection_members(tree, command, start, end, section)?;
226:     if members.is_empty() { return None; }
227:     let node_id = members[0].0.clone();
228:     let intent = SelectionIntent { start_line: start, end_line: end };
229:     use sha2::{Digest, Sha256};
230:     let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&(command, intent, &members)).ok()?));
231:     Some(ActionTarget { node_id, mutation_hash: hash, selection: Some(intent) })
232: }
233:
234: /// Derive the exact constituent content targets bound by a displayed selection.
235: fn selection_members(tree: &crate::tree::Tree, command: &str, start: u32, end: u32, section: &str) -> Option<Vec<(String, String)>> {
236:     use crate::tree::NodeKind;
237:     tree.nodes.iter().filter(|node| {
238:         if node.section.as_deref() != Some(section) { return false; }
239:         let included = match node.kind {
240:             NodeKind::File => start as usize <= node.start_line && node.start_line <= end as usize,
241:             NodeKind::Hunk => start as usize <= node.end_line && node.start_line <= end as usize,
242:             _ => false,
243:         };
244:         included
245:     }).map(|node| {
246:         let mut target = offer_target(tree, command, &node.id)?;
247:         target.selection = None;
248:         Some((node.id.clone(), target.mutation_hash))
249:     }).collect()
250: }
251:
252: /// Re-derive an offered target from fresh Git state without using fresh rows.
253: pub fn target_is_current(repo: &GitRepo, tree: &crate::tree::Tree, command: &str, target: &ActionTarget) -> Result<bool> {
254:     if let Some(intent) = target.selection {
255:         let Some(expected) = offer_selection_target(tree, command, intent.start_line, intent.end_line) else { return Ok(false); };
256:         if expected != *target { return Ok(false); }
257:         let section = if command == "status.unstage" { "staged" } else { "unstaged" };
258:         let Some(members) = selection_members(tree, command, intent.start_line, intent.end_line, section) else { return Ok(false); };
259:         for (node_id, _) in &members {
260:             let Some(member) = offer_target(tree, command, node_id) else { return Ok(false); };
261:             if !target_is_current(repo, tree, command, &member)? { return Ok(false); }
262:         }
263:         use sha2::{Digest, Sha256};
264:         let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&(command, intent, &members)).unwrap_or_default()));
265:         return Ok(hash == target.mutation_hash);
266:     }
267:     use crate::tree::MutationContent;
268:     let Some(node) = tree.node(&target.node_id) else { return Ok(false); };
269:     if offer_target(tree, command, &node.id).as_ref() != Some(target) { return Ok(false); }
270:     if node.kind == crate::tree::NodeKind::Section {
271:         return Ok(target.selection.is_none());
272:     }
273:     let Some(captured) = file_identity(tree, node) else { return Ok(false); };
274:     let Some(path) = node.path.as_deref() else { return Ok(false); };
275:     let identity = repo.file_mutation_identity(path, captured.original.as_ref().map(|(path, _)| path.as_str()), command != "status.unstage")?;
276:     let hunks;
277:     let hunk = match node.mutation.as_ref() {
278:         Some(MutationContent::Hunk { hunk, .. }) => {
279:             hunks = repo.file_hunks(path, node.section.as_deref() == Some("staged"))?;
280:             let Some(current) = hunks.iter().find(|current| current.id == hunk.id) else { return Ok(false); };
281:             Some((current, hunks.iter().filter(|other| other.same_content(current)).count()))
282:         }
283:         Some(MutationContent::File(_)) => None,
284:         _ => return Ok(false),
285:     };
286:     Ok(mutation_hash(command, identity, hunk) == target.mutation_hash)
287: }
288:
289: /// Capture cursor/selection targets from displayed structure, not rendered text.
290: pub fn displayed_targets(tree: &crate::tree::Tree, command: &str, cursor: Option<Cursor>, selection: Option<Selection>) -> Vec<ActionTarget> {
291:     use crate::tree::NodeKind;
292:     let mut targets = Vec::new();
293:     if let Some(selection) = selection {
294:         let start = selection.start_line.min(selection.end_line);
295:         let end = selection.start_line.max(selection.end_line);
296:         let section = if command == "status.unstage" { "staged" } else { "unstaged" };
297:         if tree.line_nodes.get(start).and_then(|line| line.section.as_deref()) != Some(section) { return targets; }
298:         for node in &tree.nodes {
299:             if node.section.as_deref() != Some(section) { continue; }
300:             let selected = match node.kind {
301:                 NodeKind::File => start <= node.start_line && node.start_line <= end,
302:                 NodeKind::Hunk => start <= node.end_line && node.start_line <= end,
303:                 _ => false,
304:             };
305:             if selected {
306:                 if let Some(target) = offer_target(tree, command, &node.id) { targets.push(target); }
307:             }
308:         }
309:     } else if let Some(node) = cursor.and_then(|cursor| tree.line_nodes.get(cursor.line)).and_then(|line| tree.node(&line.node)) {
310:         if let Some(target) = offer_target(tree, command, &node.id) { targets.push(target); }
311:     }
312:     targets
313: }
314:
315: /// The command the user invoked on the row under the cursor.
```

### Semantic token legend: Rust, Neovim, VSCode side by side

Rust legend order/types and modifier bit order are `src/semantic_tokens.rs:11-45`; Neovim dynamically links type groups from server legend and explicit modifiers in `lua/magit-lsp/highlights.lua:10-25,66-78`; VSCode token types and scopes are contributed at `clients/vscode/package.json:51-119` (exact excerpt below). `not contributed` means absent from the package contribution, not absent from the Rust legend.

| Rust legend token type | Rust suggested group (`TOKEN_GROUPS`) | Neovim link | VSCode token type contribution | VSCode scope contribution |
|---|---|---|---|---|
| `magit.section` | `Title` | `@lsp.type.magit.section.magit` → `Title` (server legend; override possible) | `magit-section` | `entity.name.section` |
| `magit.count` | `Comment` | `@lsp.type.magit.count.magit` → `Comment` (server legend; override possible) | not contributed | `constant.numeric` |
| `magit.file` | `Directory` | `@lsp.type.magit.file.magit` → `Directory` (server legend; override possible) | not contributed | `string` |
| `magit.file.modified` | `Changed` | `@lsp.type.magit.file.modified.magit` → `Changed` (server legend; override possible) | not contributed | `markup.changed` |
| `magit.file.added` | `Added` | `@lsp.type.magit.file.added.magit` → `Added` (server legend; override possible) | not contributed | `markup.inserted` |
| `magit.file.deleted` | `Removed` | `@lsp.type.magit.file.deleted.magit` → `Removed` (server legend; override possible) | not contributed | `markup.deleted` |
| `magit.file.renamed` | `Changed` | `@lsp.type.magit.file.renamed.magit` → `Changed` (server legend; override possible) | not contributed | `markup.changed` |
| `magit.file.untracked` | `Comment` | `@lsp.type.magit.file.untracked.magit` → `Comment` (server legend; override possible) | not contributed | `comment` |
| `magit.hunk.header` | `Special` | `@lsp.type.magit.hunk.header.magit` → `Special` (server legend; override possible) | not contributed | `meta.diff.range` |
| `magit.diff.added` | `@diff.plus` | `@lsp.type.magit.diff.added.magit` → `@diff.plus` (server legend; override possible) | `magit-diff-added` | `markup.inserted` |
| `magit.diff.removed` | `@diff.minus` | `@lsp.type.magit.diff.removed.magit` → `@diff.minus` (server legend; override possible) | `magit-diff-removed` | `markup.deleted` |
| `magit.menu.title` | `Title` | `@lsp.type.magit.menu.title.magit` → `Title` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.group` | `Statement` | `@lsp.type.magit.menu.group.magit` → `Statement` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.key` | `Keyword` | `@lsp.type.magit.menu.key.magit` → `Keyword` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.label` | `Normal` | `@lsp.type.magit.menu.label.magit` → `Normal` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.flag` | `Special` | `@lsp.type.magit.menu.flag.magit` → `Special` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.flag.on` | `Special` | `@lsp.type.magit.menu.flag.on.magit` → `Special` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.flag.off` | `MagitMenuFlagOff` | `@lsp.type.magit.menu.flag.off.magit` → `MagitMenuFlagOff` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.switch.on` | `String` | `@lsp.type.magit.menu.switch.on.magit` → `String` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.switch.off` | `Comment` | `@lsp.type.magit.menu.switch.off.magit` → `Comment` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.option` | `String` | `@lsp.type.magit.menu.option.magit` → `String` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.value` | `String` | `@lsp.type.magit.menu.value.magit` → `String` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.planned` | `Comment` | `@lsp.type.magit.menu.planned.magit` → `Comment` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.disabled` | `MagitMenuDisabled` | `@lsp.type.magit.menu.disabled.magit` → `MagitMenuDisabled` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.pending` | `MagitMenuPending` | `@lsp.type.magit.menu.pending.magit` → `MagitMenuPending` (server legend; override possible) | not contributed | not contributed |
| `magit.menu.preview` | `Comment` | `@lsp.type.magit.menu.preview.magit` → `Comment` (server legend; override possible) | not contributed | not contributed |
| `magit.message.error` | `ErrorMsg` | `@lsp.type.magit.message.error.magit` → `ErrorMsg` (server legend; override possible) | `magit-message-error` | `invalid` |
| `magit.message.warn` | `WarningMsg` | `@lsp.type.magit.message.warn.magit` → `WarningMsg` (server legend; override possible) | `magit-message-warn` | `markup.warning` |

| Rust legend modifier | Neovim modifier link | VSCode contribution |
|---|---|---|
| `staged` | `@lsp.mod.staged.magit` → `Bold` | not contributed |
| `selected` | `@lsp.mod.selected.magit` → `Visual` | not contributed |
| `planned` | `@lsp.mod.planned.magit` → `Comment` | not contributed |
| `disabled` | `@lsp.mod.disabled.magit` → `Comment` | not contributed |
| `bold` | `@lsp.mod.bold.magit` → `Bold` | not contributed |

### Rust semantic token legend and group arrays

`src/semantic_tokens.rs:11-99` (verbatim, source line numbers):

```text
11: /// Token types the status view can emit, in legend order.
12: pub const TOKEN_TYPES: &[&str] = &[
13:     "magit.section",
14:     "magit.count",
15:     "magit.file",
16:     "magit.file.modified",
17:     "magit.file.added",
18:     "magit.file.deleted",
19:     "magit.file.renamed",
20:     "magit.file.untracked",
21:     "magit.hunk.header",
22:     "magit.diff.added",
23:     "magit.diff.removed",
24:     "magit.menu.title",
25:     "magit.menu.group",
26:     "magit.menu.key",
27:     "magit.menu.label",
28:     "magit.menu.flag",
29:     "magit.menu.flag.on",
30:     "magit.menu.flag.off",
31:     "magit.menu.switch.on",
32:     "magit.menu.switch.off",
33:     "magit.menu.option",
34:     "magit.menu.value",
35:     "magit.menu.planned",
36:     "magit.menu.disabled",
37:     "magit.menu.pending",
38:     "magit.menu.preview",
39:     "magit.message.error",
40:     "magit.message.warn",
41: ];
42:
43: /// Token modifiers, in legend bit order.
44: /// keep-for: TASKS §3.15; selected is produced by log selection, while planned/disabled and bold retain recorded presentation uses (R-5.3).
45: pub const TOKEN_MODIFIERS: &[&str] = &["staged", "selected", "planned", "disabled", "bold"];
46:
47: /// Suggested theme group per token type, in [`TOKEN_TYPES`] order.
48: /// keep-for: TASKS §3.15 recorded type-to-theme contract; Lua consumes these suggestions dynamically.
49: ///
50: /// Clients link `@lsp.type.<token_type>.<filetype>` to these groups, so a new
51: /// token type needs no client change. The names are colourscheme groups except
52: /// the `Magit*` intermediates the Neovim client defines locally.
53: pub const TOKEN_GROUPS: &[&str] = &[
54:     "Title",
55:     "Comment",
56:     "Directory",
57:     "Changed",
58:     "Added",
59:     "Removed",
60:     "Changed",
61:     "Comment",
62:     "Special",
63:     "@diff.plus",
64:     "@diff.minus",
65:     "Title",
66:     "Statement",
67:     "Keyword",
68:     "Normal",
69:     "Special",
70:     "Special",
71:     "MagitMenuFlagOff",
72:     "String",
73:     "Comment",
74:     "String",
75:     "String",
76:     "Comment",
77:     "MagitMenuDisabled",
78:     "MagitMenuPending",
79:     "Comment",
80:     "ErrorMsg",
81:     "WarningMsg",
82: ];
83:
84: /// The legend advertised in `semanticTokensProvider`.
85: pub fn legend() -> SemanticTokensLegend {
86:     SemanticTokensLegend {
87:         token_types: TOKEN_TYPES.iter().copied().map(Into::into).collect(),
88:         token_modifiers: TOKEN_MODIFIERS.iter().copied().map(Into::into).collect(),
89:     }
90: }
91:
92: /// The server-suggested theme group for each token type, keyed by type.
93: pub fn token_groups() -> serde_json::Value {
94:     let mut groups = serde_json::Map::new();
95:     for (token_type, group) in TOKEN_TYPES.iter().zip(TOKEN_GROUPS) {
96:         groups.insert((*token_type).to_string(), (*group).into());
97:     }
98:     serde_json::Value::Object(groups)
99: }
```

### Neovim token/modifier links

`lua/magit-lsp/highlights.lua:10-25` (verbatim, source line numbers):

```text
10: --- Client-only overrides, token type to theme group.
11: ---
12: --- The server's suggestion wins for every type absent here, so this table is
13: --- only for local preferences or a temporarily bad server suggestion.
14: M.OVERRIDES = {}
15:
16: --- Modifier groups are applied above the type, so they should only add
17: --- attributes (or a background) and never replace a type's foreground.
18: M.MODIFIER_LINKS = {
19:   -- keep-for: TASKS §3.15 recorded modifier styling; selected is used by log selection, remaining links support planned/ref presentation (R-5.3).
20:   staged = 'Bold',
21:   selected = 'Visual',
22:   planned = 'Comment',
23:   disabled = 'Comment',
24:   bold = 'Bold',
25: }
```

### Neovim legend-driven token links

`lua/magit-lsp/highlights.lua:66-79` (verbatim, source line numbers):

```text
66:   local legend = M.legend or {}
67:   local suggested = legend.tokenTypeGroups or {}
68:   for _, token_type in ipairs(legend.tokenTypes or {}) do
69:     local group = M.OVERRIDES[token_type] or suggested[token_type]
70:     if group then
71:       local name = string.format('@lsp.type.%s.%s', token_type, ft)
72:       vim.api.nvim_set_hl(0, name, { link = group, default = true })
73:     end
74:   end
75:   for modifier, link in pairs(M.MODIFIER_LINKS) do
76:     local group = string.format('@lsp.mod.%s.%s', modifier, ft)
77:     vim.api.nvim_set_hl(0, group, { link = link, default = true })
78:   end
79: end
```

### VSCode semantic contributions

`clients/vscode/package.json:51-127` (verbatim, source line numbers):

```text
51:     "semanticTokenTypes": [
52:       {
53:         "id": "magit-section",
54:         "superType": "namespace",
55:         "description": "Status section heading"
56:       },
57:       {
58:         "id": "magit-diff-added",
59:         "description": "Added diff line"
60:       },
61:       {
62:         "id": "magit-diff-removed",
63:         "description": "Removed diff line"
64:       },
65:       {
66:         "id": "magit-message-error",
67:         "description": "Error output from a failed command"
68:       },
69:       {
70:         "id": "magit-message-warn",
71:         "description": "Warning output from a successful command"
72:       }
73:     ],
74:     "semanticTokenScopes": [
75:       {
76:         "language": "magit",
77:         "scopes": {
78:           "magit-section": [
79:             "entity.name.section"
80:           ],
81:           "magit-count": [
82:             "constant.numeric"
83:           ],
84:           "magit-file": [
85:             "string"
86:           ],
87:           "magit-file-modified": [
88:             "markup.changed"
89:           ],
90:           "magit-file-added": [
91:             "markup.inserted"
92:           ],
93:           "magit-file-deleted": [
94:             "markup.deleted"
95:           ],
96:           "magit-file-renamed": [
97:             "markup.changed"
98:           ],
99:           "magit-file-untracked": [
100:             "comment"
101:           ],
102:           "magit-hunk-header": [
103:             "meta.diff.range"
104:           ],
105:           "magit-diff-added": [
106:             "markup.inserted"
107:           ],
108:           "magit-diff-removed": [
109:             "markup.deleted"
110:           ],
111:           "magit-message-error": [
112:             "invalid"
113:           ],
114:           "magit-message-warn": [
115:             "markup.warning"
116:           ]
117:         }
118:       }
119:     ],
120:     "commands": [
121:       {
122:         "command": "magit.confirmEdit",
123:         "title": "Magit: Confirm Commit Draft",
124:         "enablement": "resourceScheme == magit-commit",
125:         "icon": "$(check)"
126:       },
127:       {
```

### Whole-crate serde attribute/custom-Serialize inventory (complete grep output)

```text
src/view.rs:60:impl Serialize for ViewKind {
src/protocol.rs:18:#[serde(tag = "type", rename_all = "camelCase")]
src/protocol.rs:24:        #[serde(skip_serializing_if = "Option::is_none")]
src/protocol.rs:26:        #[serde(skip_serializing_if = "Option::is_none")]
src/protocol.rs:134:#[serde(tag = "type", rename_all_fields = "camelCase")]
src/protocol.rs:185:    #[serde(skip_serializing_if = "Option::is_none")]
src/protocol.rs:187:    #[serde(skip_serializing_if = "Option::is_none")]
src/protocol.rs:189:    #[serde(skip_serializing_if = "Option::is_none")]
src/protocol.rs:191:    #[serde(skip_serializing_if = "Option::is_none")]
src/protocol.rs:199:    #[serde(skip_serializing_if = "Option::is_none")]
src/protocol.rs:487:#[serde(untagged)]
src/protocol.rs:495:#[serde(untagged)]
src/git.rs:63:    #[serde(skip_serializing_if = "Option::is_none")]
src/git.rs:87:    #[serde(skip_serializing_if = "Option::is_none")]
src/git.rs:89:    #[serde(skip_serializing_if = "Option::is_none")]
src/git.rs:91:    #[serde(skip_serializing_if = "Option::is_none")]
src/git.rs:93:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:109:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:111:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:115:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:117:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:119:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:122:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:125:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:131:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:157:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:159:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:163:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:165:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:167:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:170:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:173:    #[serde(skip_serializing_if = "Option::is_none")]
src/tree.rs:192:    #[serde(skip_serializing_if = "Option::is_none")]
src/menu.rs:29:#[serde(tag = "status", rename_all = "camelCase")]
src/menu.rs:41:#[serde(tag = "kind", rename_all = "camelCase")]
src/menu.rs:115:    #[serde(serialize_with = "serialize_items")]
src/menu.rs:122:        #[serde(flatten)]
src/editor.rs:56:#[serde(untagged)]
src/errors.rs:23:    #[serde(skip_serializing_if = "Option::is_none")]
src/errors.rs:26:    #[serde(skip_serializing_if = "Option::is_none")]
src/errors.rs:29:    #[serde(skip_serializing_if = "Option::is_none")]
src/errors.rs:32:    #[serde(skip_serializing_if = "Option::is_none")]
src/dispatch.rs:48:#[serde(untagged)]
```

Cargo serde dependency: `Cargo.toml:22: serde = { version = "1.0", features = ["derive"] }`. `docs/plain-support.md` is not present.
