# Harness and client-abstraction evidence (A–E)

Scope: read-only source/evidence inventory. Paths and line numbers refer to this
checkout. “Implemented” below means present in source, not proof of a particular
native-client runtime. No trace is synthesized. Home directory and username are
redacted from paths. Source commands and verbatim numbered evidence are recorded
in each section. This is an evidence report, not a redesign or a claim that the
unfinished Plain milestone is complete. No production or test files were changed;
runtime gaps and evidence that could not be obtained are listed at the end.

## A. Neovim harness

### Harness and evidence commands

Commands used for this section (paths are repository-relative; line-numbered
output came from `nl -ba`/`grep -n`):

```
wc -l scripts/ui lua/magit-lsp/debug.lua Makefile tests/common/fake_editor.rs tests/plain_harness.rs tests/common/session.rs ui-test-plan.md tests/ui/plain_init.lua scripts/color_spike.lua
cat scripts/ui; cat tests/common/fake_editor.rs; cat tests/plain_harness.rs
nl -ba tests/common/session.rs | sed -n '1,130p;690,750p'
sed -n '241,289p' ui-test-plan.md; sed -n '320,370p' ui-test-plan.md
grep -nE '^(def |class |    def )|^@' scripts/ui_lib/*.py
grep -nE '^(local function|function M\.|M\.[[:alnum:]_]+ =|local M)' lua/magit-lsp/debug.lua
```

Counts: `scripts/ui` 17, `lua/magit-lsp/debug.lua` 486, Makefile 45,
`fake_editor.rs` 96, `plain_harness.rs` 169, `session.rs` 743, `ui-test-plan.md`
370, `plain_init.lua` 32, `color_spike.lua` 413 lines. `scripts/ui` is the
complete 17-line Python entry point: it disables bytecode writes, adds its own
directory to `sys.path`, imports `ui_lib.cli.main`, and exits with its result
(`scripts/ui:1-17`). The user-facing command table is `ui-test-plan.md:241-254`;
the CLI usage and fixture inventory are verbatim at `scripts/ui_lib/cli.py:12-24`.
The implementation index from `grep` is: `cli.py` `_in_ci` 26, `_take_run` 31,
`_take_value` 53, `_start` 75, `_keys` 96, `_wait` 105, `_screen` 125,
`_state` 138, `_trace` 147, `_stop` 165, `_run` 173, `main` 199;
`driver.py` `Run` 46, `_tmux` 166, `_nvim` 179, `snapshot` 237,
`state_json` 248, `idle` 256, `document` 351, `goto_text` 369,
`wait_prompt` 398, `wait_idle` 412, `capture` 434, `wait_screen` 445,
`send_keys` 541, `_cleanup` 614, `start` 732, `stop` 844, `read_trace` 880;
`scenario.py` parser functions 117-337, assertion functions 454-620,
`evaluate` 682, `combinations` 732, `_run_combo` 851, `run` 1095.

`fake_editor.rs` is fully inspected (`cat` above). Verbatim declarations:
`tests/common/fake_editor.rs:8: pub enum Profile { Standard, DecomposedChanges, DebouncedDidChange, ReorderEcho, NoDocumentChanges, NoShowDocument, CrlfNormalise, Utf16 }`;
`tests/common/fake_editor.rs:15: pub struct FakeEditor { pub documents: HashMap<String, Document>, pub profile: Profile, pub encoding: &'static str, pending: Vec<Value>, pub outbound: Vec<Value> }`.
The model opens/edits/saves/closes, creates code-action/execute-command frames,
validates and atomically applies workspace edits, responds to applyEdit,
showDocument and workDoneProgress/create, and dispatches applyEdit echoes in
profile-dependent order (`fake_editor.rs:17-84`). Profiles model decomposed
changes, deferred didChange, response/echo reorder, missing documentChanges,
failed showDocument, CRLF normalization and UTF-16; these are fake client
behaviors, not assertions about all editors.

`tests/plain_harness.rs` was fully inspected. `ClientPump` signatures/body are
at lines 11-34: `new` initializes a spawned server with UTF-8/16/32 negotiation,
`send` rewrites request IDs and writes frames, `dispatch` writes client responses
and echoes, `step` receives server frames, dispatches requests, tracks responses
and notifications and can send progress cancellation, `until`/`until_response`
pump to conditions, and `finish` sends shutdown/exit and checks server
termination. Test index: real binary lifecycle/unsupported methods 37; profiles
54; versioned atomic multi-change 68; overlap/malformed atomic rejection 74;
UTF-16/CRLF 78; reconstructed trace JSONL 80;
`original_snapshot_order_and_atomic_rejection_matrix` 82; encoding bounds 105;
25 sequential decomposed ranges 115; deferred echo over real transport 127;
shared dispatcher send ordering 139; live binary progress requests/cancellation
while command runs 152. This is test code, not evidence this report ran it or
proved native compatibility. In particular, actual pump services server
requests `workspace/applyEdit`, `window/showDocument`, and
`window/workDoneProgress/create`; the fake dispatch fallback for unknown
methods returns -32601 (`fake_editor.rs:41-65`). `applyEdit` validates
`documentChanges` versus `changes`, unsupported resource operations, duplicate
URIs, document versions, edit shape/range/overlap, and commits all validated
replacement documents only on success (`fake_editor.rs:25-39`). Decomposition
and response/echo ordering are generated in fake `dispatch` (`:45-65`) and the
real-binary tests at `plain_harness.rs:115,127,139` assert sequential replay,
debounce flush, and actual pipe send ordering respectively.

The in-process `TestSession` is distinct. `session.rs:1-9,56-57` documents a
real `Session` and production key/invoke/ack/prompt entry points. Signature
index: constructors `new`/`with_config` 78/89, `open_status` 110,
`open_status_at` 115, `open_status_view` 126, `open_view` 132,
`open_view_with_args` 150, acknowledgements 229/234, key presses 448-520,
invokes 543-631, `set_mode` 656; second impl's `new_at` 740. `press` calls
`press_full` with current cursor/version (`:448-458`); `press_full` calls
`Session::key` with `ViewKeyParams` and tracks result (`:459-483`);
`invoke_full` calls `Session::invoke` and tracks result (`:619-629`). `ack` /
`ack_view` call `Session::rendered` (`:229-247`). `track_result` applies renders,
tracks opened views and focus (`:708-729`). This is in-process, not the
real-binary client pump.

### UI DSL, native paths and examples

The complete A.5 section content is `ui-test-plan.md:320-353`; the following
separator is line 355 and the next same-level heading is Appendix B at line 357. Header fields: `fixture`, optional `size`,
`profile`, `xfail`, `env`. By default both sizes 100x30 and 80x24 run, and both
`standard` and `vim` profiles run; `size-only: true` and `profile-only: true`
restrict the respective dimension. Body statements are `keys`, `goto`, `wait`,
`expect`, `sh:`, and `snapshot`. Assertions: screen contains/not-contains/
matches; dotted snapshot value equality; `no_message` regex; `style` (`dim`,
`bold`, `coloured`, `plain`); cursor-on; and latency(key) <= duration. The DSL
states literal `goto` is RPC (not a key press/trace latency), shell steps run in
the fixture repo with deterministic Git env and `/dev/null` stdin, and snapshots
write `.screen` and `.ansi`. `xfail` is optional; classification code is
`scenario.py:756`; parsing and combinations are `scenario.py:337,732`.

Complete representative scenario bodies (`cat`):

```
tests/ui/scenarios/commit-menu-shows-actions.scn:
# Bug 10: the commit menu must show its actions, not only its arguments.
fixture: modified-two-files
---
keys: c
wait
expect screen contains "Commit"
expect screen contains "Extend"
expect screen contains "Amend"
expect screen contains "Reword"
expect screen contains "Fixup"
expect screen contains "Squash"
snapshot: commit-menu

tests/ui/scenarios/visual-selection-identities.scn:
# A03: real visual selections below a collapsed preceding hunk.
fixture: clean
profile: vim
profile-only: true
---
wait
sh: seq 1 40 | sed 's/^/line/' > f.txt; git add f.txt; git commit -qm 'selection base'; sed -i '3aEARLY' f.txt; sed -i '/^line33$/aSELECTED\nNEIGHBOR' f.txt
wait screen contains "Unstaged changes (1)"
goto "@@"
keys: Tab
wait
expect screen not-contains "+EARLY"
snapshot: preceding-hunk-collapsed
goto "+SELECTED"
keys: V s Escape
wait
expect screen contains "Staged changes (1)"
sh: git show :f.txt | grep -qx SELECTED; ! git show :f.txt | grep -Eq 'EARLY|NEIGHBOR'; grep -qx EARLY f.txt; grep -qx NEIGHBOR f.txt
snapshot: exact-line-staged
sh: git add f.txt
keys: G
wait screen contains "Unstaged changes (0)"
goto "@@"
keys: Tab
wait
expect screen not-contains "+EARLY"
goto "+SELECTED"
keys: V u Escape
wait
goto "Unstaged changes"
expect screen contains "Unstaged changes (1)"
sh: ! git show :f.txt | grep -qx SELECTED; git show :f.txt | grep -qx EARLY; git show :f.txt | grep -qx NEIGHBOR; grep -qx SELECTED f.txt
snapshot: exact-line-unstaged
```

Individually path:line-numbered complete source excerpts (from `nl -ba`):

```text
tests/ui/scenarios/commit-menu-shows-actions.scn
   1: # Bug 10: the commit menu must show its actions, not only its arguments.
   2: fixture: modified-two-files
   3: ---
   4: keys: c
   5: wait
   6: expect screen contains "Commit"
   7: expect screen contains "Extend"
   8: expect screen contains "Amend"
   9: expect screen contains "Reword"
  10: expect screen contains "Fixup"
  11: expect screen contains "Squash"
  12: snapshot: commit-menu

tests/ui/scenarios/visual-selection-identities.scn
   1: # A03: real visual selections below a collapsed preceding hunk.
   2: fixture: clean
   3: profile: vim
   4: profile-only: true
   5: ---
   6: wait
   7: sh: seq 1 40 | sed 's/^/line/' > f.txt; git add f.txt; git commit -qm 'selection base'; sed -i '3aEARLY' f.txt; sed -i '/^line33$/aSELECTED\nNEIGHBOR' f.txt
   8: wait screen contains "Unstaged changes (1)"
   9: goto "@@"
  10: keys: Tab
  11: wait
  12: expect screen not-contains "+EARLY"
  13: snapshot: preceding-hunk-collapsed
  14: goto "+SELECTED"
  15: keys: V s Escape
  16: wait
  17: expect screen contains "Staged changes (1)"
  18: sh: git show :f.txt | grep -qx SELECTED; ! git show :f.txt | grep -Eq 'EARLY|NEIGHBOR'; grep -qx EARLY f.txt; grep -qx NEIGHBOR f.txt
  19: snapshot: exact-line-staged
  20: sh: git add f.txt
  21: keys: G
  22: wait screen contains "Unstaged changes (0)"
  23: goto "@@"
  24: keys: Tab
  25: wait
  26: expect screen not-contains "+EARLY"
  27: goto "+SELECTED"
  28: keys: V u Escape
  29: wait
  30: goto "Unstaged changes"
  31: expect screen contains "Unstaged changes (1)"
  32: sh: ! git show :f.txt | grep -qx SELECTED; git show :f.txt | grep -qx EARLY; git show :f.txt | grep -qx NEIGHBOR; grep -qx SELECTED f.txt
  33: snapshot: exact-line-unstaged

tests/ui/plain_init.lua
   1: -- Standalone standard-LSP init for the Plain native smoke. No project Lua modules.
   2: vim.opt.swapfile = false
   3: vim.opt.undofile = false
   4: vim.opt.shortmess:append('I')
   5: vim.g.plain_smoke_rich_notifications = 0
   6: vim.lsp.log.set_level('trace')
   7: vim.lsp.handlers['view/render'] = function()
   8:   vim.g.plain_smoke_rich_notifications = vim.g.plain_smoke_rich_notifications + 1
   9: end
  10:
  11: local repo = vim.env.PLAIN_REPO
  12: local server = vim.env.PLAIN_SERVER
  13: assert(repo and server)
  14:
  15: vim.api.nvim_create_autocmd('VimEnter', {
  16:   once = true,
  17:   callback = function()
  18:     local capabilities = vim.lsp.protocol.make_client_capabilities()
  19:     -- Stock Neovim executes server commands but omits this optional capability
  20:     -- object by default. Declare static support, not dynamic registration.
  21:     capabilities.workspace.executeCommand = { dynamicRegistration = false }
  22:     local client_id = vim.lsp.start({
  23:       name = 'magit-lsp-plain-smoke',
  24:       cmd = { server },
  25:       root_dir = repo,
  26:       capabilities = capabilities,
  27:     })
  28:     assert(client_id, 'vim.lsp.start did not create a client')
  29:     vim.g.plain_smoke_client_id = client_id
  30:     vim.lsp.buf_attach_client(0, client_id)
  31:   end,
  32: })
```

The menu scenario asserts server-rendered menu labels. The selection scenario
uses actual fixture Git state: stage SELECTED while EARLY/NEIGHBOR stay
unstaged, then unstage SELECTED while neighbors remain staged; its shell checks
inspect index/worktree, not rendered text. `tests/ui/plain_init.lua:1-32` is
standalone `vim.lsp.start`: trace logging, swap/undo disabled, repo/server from
environment, static `capabilities.workspace.executeCommand={dynamicRegistration=false}`
and `vim.lsp.buf_attach_client`; it does not load the project Lua client.

Actual tmux/RPC distinction: `driver.py:434-444` captures screen with `tmux
capture-pane -p` (or `-e` ANSI); `decoder.py:1-12` describes decoding escape
sequences into styled runs. `send_keys` uses tmux (`driver.py:541`); state,
snapshot, document, goto, idle and prompt probes use Neovim RPC
(`driver.py:189-200,237-261,351-382,393-432`). Thus `state`, `wait`, `goto`,
and shell assertions do not themselves capture screen; `screen`, `wait_screen`,
screen/style/cursor assertions and snapshots depend on captured pane text.
Key delivery is physical tmux; RPC helpers are separately headless. `_tmux` /
`_nvim` are `driver.py:166-186`; `start` environment and trace setup
`:732-842`; tmux options/dimensions `ui-test-plan.md:252-254`.

Full `debug.lua` (486 lines) inspected. Export/API line index from grep:
module and tables 9-34; `trace` 60, `record_notification` 81, `configure` 90,
view open/close 125/142, render received/applied/acked 158/165/176, keymap
186, pending 220, trace IDs/key events 229-286, progress events 243-273,
request/queued/prompt counters 286-316, scheduled callback accounting 317,
`idle` 330; private active-view/window/messages helpers 358-417; `snapshot`
418. It reports client-held state rather than owning server tree content. A.2
snapshot shape is `ui-test-plan.md:290-309`, A.3 idle definition `:312-314`,
A.4 trace event format `:316-318`.

Fresh owned truecolor test: made a uniquely named detached tmux session, printed
SGR `38;2;18;52;86mCOLOR`, ran Python
`subprocess.run(['tmux','capture-pane','-p','-e','-t',name], stdout=PIPE, stderr=PIPE)`
and killed only that session. Capture exit 0, stderr empty; exact repr:
`b'\x1b[38;2;18;52;86mCOLOR\x1b[39m\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n'`;
hex starts `1b5b33383b323b31383b35323b38366d434f4c4f521b5b33396d` and is followed
by 24 `0a` bytes. Exact RGB SGR bytes were present (`has_rgb_sgr=True`). This
shows local `-e` preserves truecolor escapes, not native editor color rendering.

Makefile `color-spike` target is lines 44-45: it runs `build`, then
`nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua`.
`make color-spike` was attempted: build succeeded (`ok: build`), but Neovim exited 1 at `scripts/color_spike.lua:193` with `E5113: Lua chunk: ./scripts/color_spike.lua:193: magit-lsp setup failed: nil`; the failure log ended with that error and stack trace. The script creates the fixture/output directory before setup (`:187-188`), but failed before producing HTML/Markdown; do not treat any paths as emitted artifacts. Source-defined intended outputs: HTML pattern
`build/test-output/status-<scheme>-<background>.html` (`color_spike.lua:284-297`)
and `build/test-output/semantic-tokens.md` (`:388-400`). Script prints the
actual HTML count and legend path at line 400; no generated count is claimed.

Repository-wide bounded text search command:
`grep -RInE 'screenshot|\.png|\.jpg|\.jpeg|\.mp4|\.webm|trace\.zip|capture-pane|Popen\(|subprocess\.' --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules . 2>/dev/null | head -100`.
Matches included historical text (`REVIEW_REPORT.md`, `TASKS.md`, notes),
no-vision policy text, and helper code `scripts/ui_lib/driver.py:166,179,436`,
`scenario.py:813`, `startup.py:178`, `scripts/test_plain_neovim.py:21,106,198,227,526-532`.
Because output is capped at 100 lines this is a bounded inventory, not a proof
of absence elsewhere. No images were opened/rendered/captured; no project
harness or color spike was run. Owned tmux session was killed after capture.

### Verbatim numbered source requested

The following are full-file, line-numbered transcriptions from this checkout.
Line numbers are source-file lines.

#### `scripts/ui` — complete (17 lines)

```text
   1: #!/usr/bin/env python3
   2: """Real-UI test driver: drive Neovim inside tmux, inspect its screen.
   3:
   4: See `ui-test-plan.md` §A.1 for the command set.
   5: """
   6:
   7: import sys
   8: from pathlib import Path
   9:
  10: # Keep `build/` clean: no `__pycache__` next to the helpers.
  11: sys.dont_write_bytecode = True
  12: sys.path.insert(0, str(Path(__file__).resolve().parent))
  13:
  14: from ui_lib.cli import main  # noqa: E402
  15:
  16: if __name__ == "__main__":
  17:     raise SystemExit(main())
```

#### `lua/magit-lsp/debug.lua` — complete (486 lines)

```text
   1: --- Debug API for the UI harness (§A.2–§A.4).
   2: ---
   3: --- The module reports only state the client already holds: the views it opened,
   4: --- the render versions it applied and acknowledged, the keymap the server sent,
   5: --- and the requests, prompts and scheduled callbacks the client started itself.
   6: --- It never asks the server for anything, so a snapshot cannot change the UI it
   7: --- describes.
   8:
   9: local M = {}
  10:
  11: local uv = vim.uv or vim.loop
  12:
  13: --- Client work that is still outstanding, counted by the client itself.
  14: M.state = {
  15:   requests = 0,
  16:   prompts = 0,
  17:   scheduled = 0,
  18:   --- Requests queued locally and not answered yet.
  19:   queued = 0,
  20:   --- Progress tokens the server has begun and not ended, by token.
  21:   progress = {},
  22: }
  23:
  24: --- Registered views by id.
  25: M.views = {}
  26:
  27: --- View ids in stack order, oldest first.
  28: M.order = {}
  29:
  30: --- The last 20 notifications shown to the user.
  31: M.notifications = {}
  32:
  33: --- The last 20 progress events, including their semantic operation metadata.
  34: M.progress_events = {}
  35:
  36: --- Whether the last `idle()` call found the client busy; the trace only emits
  37: --- an `idle` event when that changes to true, so polling does not spam it.
  38: local was_idle = false
  39:
  40: local trace_path = vim.env.MAGIT_LSP_TRACE
  41: local trace_file
  42: local trace_started = uv.hrtime()
  43: --- The wall-clock time of `trace_started`, so server events can be merged.
  44: local trace_epoch
  45: do
  46:   local seconds, microseconds = uv.gettimeofday()
  47:   -- Wall clock at `trace_started`; the server merges by this absolute field.
  48:   trace_epoch = seconds * 1000 + microseconds / 1000
  49: end
  50: --- The id of the key press the following events belong to.
  51: local trace_press = 0
  52:
  53: if trace_path and trace_path ~= '' then
  54:   trace_file = io.open(vim.fn.expand(trace_path), 'a')
  55: end
  56:
  57: --- Append one trace event when `MAGIT_LSP_TRACE` names a file.
  58: ---@param event string
  59: ---@param detail table|nil
  60: function M.trace(event, detail)
  61:   if not trace_file then
  62:     return
  63:   end
  64:   local elapsed_ms = (uv.hrtime() - trace_started) / 1e6
  65:   local ok, line = pcall(vim.json.encode, {
  66:     t_ms = tonumber(string.format('%.3f', elapsed_ms)),
  67:     t_unix_ms = tonumber(string.format('%.3f', trace_epoch + elapsed_ms)),
  68:     id = trace_press,
  69:     event = event,
  70:     detail = detail or vim.NIL,
  71:   })
  72:   if not ok then
  73:     return
  74:   end
  75:   trace_file:write(line .. '\n')
  76:   trace_file:flush()
  77: end
  78:
  79: --- Keep the last 20 notifications the client displayed.
  80: ---@param msg any
  81: function M.record_notification(msg)
  82:   M.notifications[#M.notifications + 1] = tostring(msg)
  83:   if #M.notifications > 20 then
  84:     table.remove(M.notifications, 1)
  85:   end
  86: end
  87:
  88: --- Called once by the main client before it opens any view.
  89: ---@param opts table|nil
  90: function M.configure(opts)
  91:   opts = opts or {}
  92:   if M.notify_wrapped then
  93:     return
  94:   end
  95:   M.notify_wrapped = true
  96:   -- A server progress token keeps `idle` false until its `end` arrives, so a
  97:   -- detached commit cannot be observed half-finished.
  98:   vim.api.nvim_create_autocmd('LspProgress', {
  99:     callback = function(args)
 100:       local params = args.data and args.data.params
 101:       local value = params and params.value
 102:       if type(value) ~= 'table' then
 103:         return
 104:       end
 105:       if value.kind == 'begin' then
 106:         M.progress_started(params.token, value)
 107:       elseif value.kind == 'report' then
 108:         M.progress_reported(params.token, value)
 109:       elseif value.kind == 'end' then
 110:         M.progress_finished(params.token)
 111:       end
 112:     end,
 113:   })
 114:   local original = vim.notify
 115:   vim.notify = function(msg, level, notify_opts)
 116:     M.record_notification(msg)
 117:     return original(msg, level, notify_opts)
 118:   end
 119: end
 120:
 121: --- Record a view the client just opened.
 122: ---@param id string
 123: ---@param kind string
 124: ---@param buf integer|nil the view's buffer, used to find the active view
 125: function M.view_open(id, kind, buf)
 126:   if not id then
 127:     return
 128:   end
 129:   local view = M.views[id]
 130:   if not view then
 131:     view = { id = id }
 132:     M.views[id] = view
 133:     M.order[#M.order + 1] = id
 134:   end
 135:   view.kind = kind or view.kind or ''
 136:   view.buf = buf or view.buf
 137:   M.trace('window_opened', { view = id, kind = view.kind, buffer = buf })
 138: end
 139:
 140: --- Record that a view is gone.
 141: ---@param id string
 142: function M.view_close(id)
 143:   if not M.views[id] then
 144:     return
 145:   end
 146:   M.views[id] = nil
 147:   for index, value in ipairs(M.order) do
 148:     if value == id then
 149:       table.remove(M.order, index)
 150:       break
 151:     end
 152:   end
 153: end
 154:
 155: --- Record a render notification as it arrives.
 156: ---@param id string
 157: ---@param version integer
 158: function M.render_received(id, version)
 159:   M.trace('render_received', { view = id, version = version })
 160: end
 161:
 162: --- Record a render the client actually applied to a buffer.
 163: ---@param id string
 164: ---@param version integer
 165: function M.render_applied(id, version)
 166:   local view = M.views[id]
 167:   if view then
 168:     view.version = version
 169:   end
 170:   M.trace('render_applied', { view = id, version = version })
 171: end
 172:
 173: --- Record the version the client told the server it holds.
 174: ---@param id string
 175: ---@param version integer
 176: function M.render_acked(id, version)
 177:   local view = M.views[id]
 178:   if view then
 179:     view.acked = version
 180:   end
 181: end
 182:
 183: --- Record the keymap the server sent, with the keys the client can press.
 184: ---@param id string
 185: ---@param notice table `view/keymap` params
 186: function M.keymap_received(id, notice)
 187:   local view = M.views[id]
 188:   if not view then
 189:     return
 190:   end
 191:   local keys = {}
 192:   local seen = {}
 193:   local live = notice.liveKeys
 194:   if type(live) == 'table' and #live > 0 then
 195:     for _, chord in ipairs(live) do
 196:       if not seen[chord] then
 197:         seen[chord] = true
 198:         keys[#keys + 1] = chord
 199:       end
 200:     end
 201:   else
 202:     for _, binding in ipairs(notice.bindings or {}) do
 203:       for _, chord in ipairs(binding.keys or {}) do
 204:         if not seen[chord] then
 205:           seen[chord] = true
 206:           keys[#keys + 1] = chord
 207:         end
 208:       end
 209:     end
 210:   end
 211:   view.scope = notice.scope or ''
 212:   view.capture = notice.capture == true
 213:   view.keys = keys
 214:   view.pending = notice.pending or ''
 215: end
 216:
 217: --- Record the server's pending menu steps for a view.
 218: ---@param id string
 219: ---@param pending string|nil
 220: function M.set_pending(id, pending)
 221:   local view = M.views[id]
 222:   if view then
 223:     view.pending = pending or ''
 224:   end
 225: end
 226:
 227: --- Start a new trace id for one key press.
 228: ---@param steps string[]
 229: function M.key_pressed(steps)
 230:   trace_press = trace_press + 1
 231:   M.trace('key_pressed', { steps = steps })
 232: end
 233:
 234: --- The id of the key press being traced, sent with `view/key` (§A.4).
 235: ---@return integer
 236: function M.trace_id()
 237:   return trace_press
 238: end
 239:
 240: --- Record a server progress token starting; `idle` waits for it to end.
 241: ---@param token string|integer
 242: ---@param value table|nil
 243: function M.progress_started(token, value)
 244:   M.state.progress[tostring(token)] = vim.deepcopy(value or {})
 245:   M.record_progress(token, value or { kind = 'begin' })
 246: end
 247:
 248: --- Retain bounded progress evidence without changing server-owned state.
 249: ---@param token string|integer
 250: ---@param value table
 251: function M.record_progress(token, value)
 252:   local event = vim.tbl_extend('force', vim.deepcopy(value), { token = tostring(token) })
 253:   M.progress_events[#M.progress_events + 1] = event
 254:   if #M.progress_events > 20 then
 255:     table.remove(M.progress_events, 1)
 256:   end
 257:   M.trace('progress', event)
 258: end
 259:
 260: --- Record feedback only for an operation that has not already ended.
 261: ---@param token string|integer
 262: ---@param value table
 263: function M.progress_reported(token, value)
 264:   local current = M.state.progress[tostring(token)]
 265:   if current then
 266:     M.state.progress[tostring(token)] = vim.tbl_extend('force', current, value)
 267:     M.record_progress(token, value)
 268:   end
 269: end
 270:
 271: --- Record a server progress token ending.
 272: ---@param token string|integer
 273: function M.progress_finished(token)
 274:   M.state.progress[tostring(token)] = nil
 275:   M.record_progress(token, { kind = 'end' })
 276: end
 277:
 278: --- Record one canonical key step being sent to the server.
 279: ---@param step string
 280: ---@param id string|nil
 281: function M.key_sent(step, id)
 282:   M.trace('key_sent', { key = step, view = id })
 283: end
 284:
 285: --- Count one asynchronous request until `M.request_finished` runs.
 286: function M.request_started()
 287:   M.state.requests = M.state.requests + 1
 288: end
 289:
 290: --- Stop counting an asynchronous request.
 291: function M.request_finished()
 292:   M.state.requests = math.max(0, M.state.requests - 1)
 293: end
 294:
 295: --- Count one request entering the client's queue.
 296: function M.queued_started()
 297:   M.state.queued = M.state.queued + 1
 298: end
 299:
 300: --- Stop counting a queued request once it has been answered or dropped.
 301: function M.queued_finished()
 302:   M.state.queued = math.max(0, M.state.queued - 1)
 303: end
 304:
 305: --- Count one prompt the client is waiting on.
 306: function M.prompt_started()
 307:   M.state.prompts = M.state.prompts + 1
 308: end
 309:
 310: --- Stop counting a prompt once it is answered or cancelled.
 311: function M.prompt_finished()
 312:   M.state.prompts = math.max(0, M.state.prompts - 1)
 313: end
 314:
 315: --- Queue `fn` on Neovim's event loop, counted as outstanding client work.
 316: ---@param fn fun()
 317: function M.schedule(fn)
 318:   M.state.scheduled = M.state.scheduled + 1
 319:   vim.schedule(function()
 320:     local ok, err = pcall(fn)
 321:     M.state.scheduled = M.state.scheduled - 1
 322:     if not ok then
 323:       error(err, 0)
 324:     end
 325:   end)
 326: end
 327:
 328: --- True when no client work is outstanding and every view is acknowledged.
 329: ---@return boolean
 330: function M.idle()
 331:   local busy = M.state.requests > 0
 332:     or M.state.queued > 0
 333:     or M.state.prompts > 0
 334:     or M.state.scheduled > 0
 335:     or next(M.state.progress) ~= nil
 336:   if not busy then
 337:     for _, id in ipairs(M.order) do
 338:       local view = M.views[id]
 339:       if view.version == nil or view.version ~= view.acked then
 340:         busy = true
 341:         break
 342:       end
 343:     end
 344:   end
 345:   if busy then
 346:     was_idle = false
 347:     return false
 348:   end
 349:   if not was_idle then
 350:     was_idle = true
 351:     M.trace('idle', { views = #M.order })
 352:   end
 353:   return true
 354: end
 355:
 356: --- The view owning the current buffer, else the top of the stack.
 357: ---@return table|nil
 358: local function active_view()
 359:   local buf = vim.api.nvim_get_current_buf()
 360:   for _, id in ipairs(M.order) do
 361:     if M.views[id].buf == buf then
 362:       return M.views[id]
 363:     end
 364:   end
 365:   local id = M.order[#M.order]
 366:   return id and M.views[id] or nil
 367: end
 368:
 369: --- One window as the snapshot reports it.
 370: ---@param win integer
 371: local function window_state(win)
 372:   local buf = vim.api.nvim_win_get_buf(win)
 373:   local config = vim.api.nvim_win_get_config(win)
 374:   local float = config.relative ~= nil and config.relative ~= ''
 375:   local row, col
 376:   if float then
 377:     row = config.row or 0
 378:     col = config.col or 0
 379:   else
 380:     local position = vim.fn.win_screenpos(win)
 381:     row = math.max(0, position[1] - 1)
 382:     col = math.max(0, position[2] - 1)
 383:   end
 384:   local width = vim.api.nvim_win_get_width(win)
 385:   return {
 386:     id = win,
 387:     float = float,
 388:     row = row,
 389:     col = col,
 390:     width = width,
 391:     height = vim.api.nvim_win_get_height(win),
 392:     -- True when this window spans the whole editor, as the status view does.
 393:     full_width = width == vim.o.columns,
 394:     buffer = buf,
 395:     filetype = vim.bo[buf].filetype,
 396:   }
 397: end
 398:
 399: --- The last 20 lines of `:messages`.
 400: local function recent_messages()
 401:   local ok, output = pcall(vim.fn.execute, 'messages')
 402:   if not ok or type(output) ~= 'string' then
 403:     return {}
 404:   end
 405:   local lines = {}
 406:   for line in output:gmatch('[^\n]+') do
 407:     lines[#lines + 1] = line
 408:   end
 409:   local recent = {}
 410:   for index = math.max(1, #lines - 19), #lines do
 411:     recent[#recent + 1] = lines[index]
 412:   end
 413:   return recent
 414: end
 415:
 416: --- The §A.2 state snapshot: JSON-encodable and read-only.
 417: ---@return table
 418: function M.snapshot()
 419:   local view = active_view()
 420:   local cursor = vim.api.nvim_win_get_cursor(0)
 421:   local windows = {}
 422:   for _, win in ipairs(vim.api.nvim_list_wins()) do
 423:     windows[#windows + 1] = window_state(win)
 424:   end
 425:   local stack = {}
 426:   for _, id in ipairs(M.order) do
 427:     stack[#stack + 1] = { id = id, kind = M.views[id].kind }
 428:   end
 429:   local drafts = {}
 430:   for _, record in pairs(require('magit-lsp.view').views) do
 431:     if record.document and vim.api.nvim_buf_is_valid(record.buf) then
 432:       drafts[#drafts + 1] = {
 433:         runId = record.id, uri = record.uri, buffer = record.buf,
 434:         version = record.version, savedVersion = record.saved_version,
 435:         modified = vim.bo[record.buf].modified, busy = record.busy == true,
 436:         disconnected = record.disconnected == true, error = record.sync_error,
 437:       }
 438:     end
 439:   end
 440:   table.sort(drafts, function(a, b) return a.uri < b.uri end)
 441:   return {
 442:     mode = vim.fn.mode(),
 443:     cursor = { row = cursor[1] - 1, col = cursor[2] },
 444:     idle = M.idle(),
 445:     windows = windows,
 446:     view_stack = stack,
 447:     drafts = drafts,
 448:     pending = (view and view.pending) or '',
 449:     render = {
 450:       version = (view and view.version) or 0,
 451:       acked = (view and view.acked) or 0,
 452:     },
 453:     keymap = {
 454:       scope = (view and view.scope) or '',
 455:       capture = (view and view.capture) or false,
 456:       keys = (view and view.keys) or {},
 457:     },
 458:     messages = recent_messages(),
 459:     notifications = vim.deepcopy(M.notifications),
 460:   }
 461: end
 462:
 463: -- Fill the product-side hook table with the real recorder. Product modules only
 464: -- require `magit-lsp.hooks`, so a normal start never loads this module; the test
 465: -- harness and `MAGIT_LSP_TRACE` do, and this is where recording turns on.
 466: local hooks = require('magit-lsp.hooks')
 467: hooks.configure = M.configure
 468: hooks.prompt_started = M.prompt_started
 469: hooks.prompt_finished = M.prompt_finished
 470: hooks.request_started = M.request_started
 471: hooks.request_finished = M.request_finished
 472: hooks.queued_started = M.queued_started
 473: hooks.queued_finished = M.queued_finished
 474: hooks.schedule = M.schedule
 475: hooks.render_received = M.render_received
 476: hooks.render_applied = M.render_applied
 477: hooks.render_acked = M.render_acked
 478: hooks.set_pending = M.set_pending
 479: hooks.keymap_received = M.keymap_received
 480: hooks.view_open = M.view_open
 481: hooks.view_close = M.view_close
 482: hooks.key_pressed = M.key_pressed
 483: hooks.trace_id = M.trace_id
 484: hooks.key_sent = M.key_sent
 485:
 486: return M
```

#### `tests/common/fake_editor.rs` — complete (96 lines)

```text
   1: //! Scripted LSP editor model used to exercise Plain-client quirks.
   2:
   3: use serde_json::{json, Value};
   4: use std::collections::HashMap;
   5:
   6: /// Client compatibility behavior toggled by a conformance test.
   7: #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   8: pub enum Profile { Standard, DecomposedChanges, DebouncedDidChange, ReorderEcho, NoDocumentChanges, NoShowDocument, CrlfNormalise, Utf16 }
   9:
  10: /// Open editor document state.
  11: #[derive(Clone, Debug)]
  12: pub struct Document { pub text: String, pub version: i64 }
  13:
  14: /// Scriptable client-side editor model, independent of production server behavior.
  15: pub struct FakeEditor { pub documents: HashMap<String, Document>, pub profile: Profile, pub encoding: &'static str, pending: Vec<Value>, pub outbound: Vec<Value> }
  16:
  17: impl FakeEditor {
  18:     /// Create a client model with the requested compatibility profile.
  19:     pub fn new(profile: Profile) -> Self { Self { documents: HashMap::new(), profile, encoding: "utf-16", pending: Vec::new(), outbound: Vec::new() } }
  20:     /// Open a document and return its didOpen notification.
  21:     pub fn open(&mut self, uri: &str, text: &str) -> Value { self.documents.insert(uri.into(), Document { text: text.into(), version: 1 }); json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"plaintext","version":1,"text":text}}}) }
  22:     /// Replace document text; debounce profile queues the notification until flush.
  23:     pub fn edit(&mut self, uri: &str, text: &str) -> Value { let d=self.documents.get_mut(uri).expect("open document"); let old=d.text.clone(); d.version+=1; d.text=text.into(); let changes=if matches!(self.profile,Profile::DecomposedChanges) { decomposed(&old,text,self.encoding) } else { vec![json!({"text":text})] }; let m=json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":d.version},"contentChanges":changes}}); if matches!(self.profile,Profile::DebouncedDidChange) { self.pending.push(m.clone()); } m }
  24:     /// Release deferred didChange notifications.
  25:     pub fn flush(&mut self) -> Vec<Value> { std::mem::take(&mut self.pending) }
  26:     /// Return whether a debounced change is waiting to be sent.
  27:     pub fn has_pending(&self) -> bool { !self.pending.is_empty() }
  28:     /// Return the current editor text for a URI.
  29:     pub fn text(&self, uri: &str) -> &str { &self.documents.get(uri).expect("open document").text }
  30:     /// Create a save notification.
  31:     pub fn save(&self, uri: &str) -> Value { json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri}}}) }
  32:     /// Close an open document.
  33:     pub fn close(&mut self, uri: &str) -> Value { self.documents.remove(uri); json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}) }
  34:     /// Send a code-action request for a range.
  35:     pub fn code_action(&self, uri: &str, range: Value) -> Value { json!({"jsonrpc":"2.0","id":71,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}) }
  36:     /// Send an execute-command request.
  37:     pub fn execute_command(&self, command: &str, arguments: Value) -> Value { json!({"jsonrpc":"2.0","id":72,"method":"workspace/executeCommand","params":{"command":command,"arguments":arguments}}) }
  38:     /// Apply an entire workspace edit atomically, rejecting stale versions and invalid ranges.
  39:     pub fn apply_edit(&mut self, edit: &Value) -> Value {
  40:         if !edit.is_object() || (edit.get("documentChanges").is_some() && edit.get("changes").is_some()) { return rejected("malformed workspace edit"); }
  41:         if edit.get("documentChanges").is_some() && matches!(self.profile, Profile::NoDocumentChanges) { return rejected("documentChanges is unsupported"); }
  42:         if edit.get("documentChanges").is_some_and(|v| !v.is_array()) { return rejected("malformed documentChanges"); }
  43:         let Some(changes) = edit["documentChanges"].as_array() else {
  44:             let Some(changes) = edit["changes"].as_object() else { return rejected("malformed workspace edit"); };
  45:             let mut replacements = Vec::new();
  46:             for (uri, edits) in changes { let Some(doc) = self.documents.get(uri) else { return rejected("document is not open"); }; let Some(edits) = edits.as_array() else { return rejected("malformed edits"); }; let Some(result) = apply_edits(&doc.text, edits, self.encoding) else { return rejected("invalid or overlapping edits"); }; replacements.push((uri.clone(), result)); }
  47:             return self.commit(replacements);
  48:         };
  49:         let mut replacements = Vec::new(); let mut seen = std::collections::HashSet::new();
  50:         for change in changes { if change.get("kind").is_some() { return rejected("resource operations are unsupported"); } let Some(uri) = change["textDocument"]["uri"].as_str() else { return rejected("malformed document change"); }; if !seen.insert(uri) { return rejected("duplicate document change"); } let Some(doc) = self.documents.get(uri) else { return rejected("document is not open"); }; let Some(version) = change["textDocument"].get("version") else { return rejected("missing version"); }; if !version.is_null() && version.as_i64()!=Some(doc.version) { return rejected("document version is stale or malformed"); } let Some(edits) = change["edits"].as_array() else { return rejected("missing edits"); }; let Some(result) = apply_edits(&doc.text, edits, self.encoding) else { return rejected("invalid or overlapping edits"); }; replacements.push((uri.to_owned(),result)); }
  51:         self.commit(replacements)
  52:     }
  53:
  54:     fn commit(&mut self, replacements: Vec<(String,String)>) -> Value { for (uri,text) in replacements { let d=self.documents.get_mut(&uri).unwrap(); d.text=if matches!(self.profile,Profile::CrlfNormalise){text.replace("\r\n","\n")}else{text}; d.version+=1; } json!({"applied":true}) }
  55:     /// Respond to supported server requests while retaining outbound order for assertions.
  56:     pub fn respond(&mut self, request: &Value) -> Value { self.outbound.push(request.clone()); match request["method"].as_str().unwrap_or("") {"workspace/applyEdit"=>json!({"jsonrpc":"2.0","id":request["id"],"result":self.apply_edit(&request["params"]["edit"])}),"window/showDocument"=>json!({"jsonrpc":"2.0","id":request["id"],"result":{"success":!matches!(self.profile,Profile::NoShowDocument)}}),"window/workDoneProgress/create"=>json!({"jsonrpc":"2.0","id":request["id"],"result":null}),_=>json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32601,"message":"Method not found"}})} }
  57:
  58:     /// Dispatch a server request and return client-to-server frames in wire order.
  59:     pub fn dispatch(&mut self, request: &Value) -> Vec<Value> {
  60:         let before = self.documents.clone();
  61:         let response = self.respond(request);
  62:         let mut frames = vec![response];
  63:         if request["method"] == "workspace/applyEdit" && frames[0]["result"]["applied"] == true {
  64:             let mut uris: Vec<_> = self.documents.keys().cloned().collect();
  65:             uris.sort();
  66:             let mut echoes = Vec::new();
  67:             for uri in uris {
  68:                 let doc = &self.documents[&uri];
  69:                 if doc.version != before[&uri].version {
  70:                     let changes = if matches!(self.profile, Profile::DecomposedChanges) {
  71:                         decomposed(&before[&uri].text, &doc.text, self.encoding)
  72:                     } else {
  73:                         vec![json!({"text":doc.text})]
  74:                     };
  75:                     let echo = json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":doc.version},"contentChanges":changes}});
  76:                     if matches!(self.profile, Profile::DebouncedDidChange) { self.pending.push(echo); } else { echoes.push(echo); }
  77:                 }
  78:             }
  79:             if matches!(self.profile, Profile::ReorderEcho) { frames.extend(echoes); } else { echoes.extend(frames); frames = echoes; }
  80:         }
  81:         frames
  82:     }
  83: }
  84:
  85: fn rejected(reason:&str)->Value { json!({"applied":false,"failureReason":reason}) }
  86: fn decomposed(old:&str,new:&str,encoding:&str)->Vec<Value>{
  87:     let line=old.bytes().filter(|b|*b==b'\n').count();
  88:     let last=old.rsplit('\n').next().unwrap().trim_end_matches('\r');
  89:     let character=match encoding{"utf-8"=>last.len(),"utf-32"=>last.chars().count(),_=>last.encode_utf16().count()};
  90:     let mut changes=vec![json!({"range":{"start":{"line":0,"character":0},"end":{"line":line,"character":character}},"text":""})];
  91:     // Each insertion is relative to the preceding result, not the old snapshot.
  92:     for (line,text) in new.split_inclusive('\n').enumerate(){changes.push(json!({"range":{"start":{"line":line,"character":0},"end":{"line":line,"character":0}},"text":text}));}
  93:     changes
  94: }
  95: fn apply_edits(text:&str, edits:&[Value], encoding:&str)->Option<String>{let mut ranges=Vec::new();for e in edits{let start=offset(text,&e["range"]["start"],encoding)?;let end=offset(text,&e["range"]["end"],encoding)?;let replacement=e["newText"].as_str()?.to_owned();if start>end{return None;}ranges.push((start,end,replacement));}ranges.sort_by_key(|(a,b,_)|(*a,*b));if ranges.windows(2).any(|w|w[0].1>w[1].0){return None;}let mut result=text.to_owned();for (a,b,repl) in ranges.into_iter().rev(){result.replace_range(a..b,&repl);}Some(result)}
  96: fn offset(text:&str, position:&Value, encoding:&str)->Option<usize>{let line=position["line"].as_u64()? as usize;let character=position["character"].as_u64()? as usize;let start=if line==0{0}else{text.match_indices('\n').nth(line-1)?.0+1};let end=text[start..].find('\n').map(|n|start+n).unwrap_or(text.len());let slice=text.get(start..end)?.strip_suffix('\r').unwrap_or(&text[start..end]);let mut units=0;for (i,c) in slice.char_indices(){if units==character{return Some(start+i)}units+=match encoding{"utf-8"=>c.len_utf8(),"utf-32"=>1,_=>c.len_utf16()};if units>character{return None}}(units==character).then_some(start+slice.len())}
```

#### `tests/plain_harness.rs` — complete (169 lines)

```text
   1: //! Plain editor conformance self-tests and real-binary protocol smoke test.
   2: #[path = "common/stdio.rs"] mod stdio;
   3: #[path = "common/fake_editor.rs"] mod fake_editor;
   4: #[path = "common/mod.rs"] mod fixtures;
   5: use fake_editor::{FakeEditor,Profile};
   6: use serde_json::{json,Value};
   7: use std::time::Duration;
   8: use std::fs;
   9:
  10: struct ClientPump { server: stdio::TestServer, editor: FakeEditor, next_id: i64, replies: std::collections::HashMap<i64,Value>, cancel_on_edit: bool }
  11: impl ClientPump {
  12:  fn new(mut server: stdio::TestServer, profile: Profile) -> Self {
  13:   let init=server.initialize(json!({"window":{"workDoneProgress":true},"general":{"positionEncodings":["utf-8","utf-16","utf-32"]}}));
  14:   assert!(init["result"]["capabilities"].is_object());
  15:   let mut editor=FakeEditor::new(profile);
  16:   editor.encoding=match init["result"]["capabilities"]["positionEncoding"].as_str().unwrap_or("utf-16") {"utf-8"=>"utf-8","utf-16"=>"utf-16","utf-32"=>"utf-32",other=>panic!("invalid negotiated encoding: {other}")};
  17:   Self{server,editor,next_id:2,replies:Default::default(),cancel_on_edit:false}
  18:  }
  19:  fn send(&mut self, mut message: Value) -> i64 { let id=self.next_id; if message.get("id").is_some(){self.next_id+=1; message["id"]=json!(id);} self.server.send(message); id }
  20:  fn dispatch(&mut self, request:&Value) { for frame in self.editor.dispatch(request){self.server.send(frame);} }
  21:  fn step(&mut self) {
  22:   let message=self.server.recv();
  23:   if message["method"].is_string() && !message["id"].is_null(){ self.dispatch(&message); return; }
  24:   if let Some(id)=message["id"].as_i64(){assert!(self.replies.insert(id,message).is_none(),"duplicate response");return;}
  25:   if self.cancel_on_edit && message["method"]=="view/effects" && message["params"]["effects"].as_array().is_some_and(|e|e.iter().any(|e|e["type"]=="editDocument")) {
  26:    let token=self.server.notifications.iter().rev().find(|m|m["method"]=="$/progress"&&m["params"]["value"]["kind"]=="begin").expect("commit began")["params"]["token"].clone();
  27:    self.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));self.cancel_on_edit=false;
  28:   }
  29:   self.server.notifications.push(message);
  30:  }
  31:  fn until(&mut self, condition:impl Fn(&Self)->bool) { let deadline=std::time::Instant::now()+Duration::from_secs(15); while !condition(self){assert!(std::time::Instant::now()<deadline,"pump condition timed out");self.step();} }
  32:  fn until_response(&mut self, id:i64)->Value {self.until(|c|c.replies.contains_key(&id));self.replies.remove(&id).unwrap()}
  33:  fn finish(mut self) {let id=self.send(json!({"jsonrpc":"2.0","id":0,"method":"shutdown","params":null}));assert_eq!(self.until_response(id)["result"],Value::Null);self.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));assert!(self.server.finish(Duration::from_secs(10)).success());}
  34: }
  35:
  36: #[test]
  37: fn real_binary_lifecycle_and_current_unsupported_methods_are_honest(){
  38:  let mut client=ClientPump::new(stdio::TestServer::spawn(&[]),Profile::Standard);
  39:  let uri="file:///tmp/plain-harness.txt";
  40:  let frame=client.editor.open(uri,"hello\n");client.send(frame);
  41:  let frame=client.editor.edit(uri,"edited\n");client.send(frame);
  42:  let frame=client.editor.save(uri);client.send(frame);
  43:  assert_eq!(client.editor.text(uri),"edited\n");
  44:  let frame=client.editor.code_action(uri,json!({"start":{"line":0,"character":0},"end":{"line":0,"character":1}}));let ca_id=client.send(frame);
  45:  let frame=client.editor.execute_command("magit.command",json!([]));let ec_id=client.send(frame);
  46:  // Await the second request first: the pump must retain the first response.
  47:  let ec=client.until_response(ec_id);assert!(ec["error"].is_object());
  48:  let ca=client.until_response(ca_id);assert!(ca.get("result").is_some()||ca["error"].is_object());
  49:  let frame=client.editor.close(uri);client.send(frame);assert!(!client.editor.documents.contains_key(uri));
  50:  client.finish();
  51: }
  52:
  53: #[test]
  54: fn all_documented_profiles_have_assertable_behavior(){
  55:  let profiles=[Profile::DecomposedChanges,Profile::DebouncedDidChange,Profile::ReorderEcho,Profile::NoDocumentChanges,Profile::NoShowDocument,Profile::CrlfNormalise,Profile::Utf16];
  56:  for p in profiles {let mut e=FakeEditor::new(p);let uri="file:///x";e.open(uri,"a\nb\n");match p {
  57:   Profile::DecomposedChanges=>{let n=e.edit(uri,"A\nB\n");let c=n["params"]["contentChanges"].as_array().unwrap();assert_eq!(c.len(),3);assert!(c.iter().all(|v|v["range"].is_object()));}
  58:  Profile::DebouncedDidChange=>{e.edit(uri,"queued");assert!(e.has_pending());let sent=e.flush();assert!(!e.has_pending());assert_eq!(sent.len(),1);assert_eq!(sent[0]["params"]["contentChanges"][0]["text"],"queued");}
  59:  Profile::ReorderEcho=>{let req=json!({"jsonrpc":"2.0","id":17,"method":"workspace/applyEdit","params":{"edit":{"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}}});let response=e.respond(&req);let echo=e.edit(uri,"echo");assert_eq!(response["id"],17);assert_eq!(response["result"]["applied"],true);assert_eq!(echo["params"]["textDocument"]["version"],3);assert_eq!(e.text(uri),"echo");assert_eq!(e.outbound[0]["method"],"workspace/applyEdit");}
  60:   Profile::NoDocumentChanges=>{assert_eq!(e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]}))["applied"],false);assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}))["applied"],true);}
  61:  Profile::NoShowDocument=>{assert_eq!(e.respond(&json!({"id":5,"method":"window/showDocument"}))["result"]["success"],false);}
  62:  Profile::CrlfNormalise=>{e.documents.get_mut(uri).unwrap().text="a\r\nb".into();e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}));assert!(!e.text(uri).contains("\r\n"));}
  63:  Profile::Utf16=>{e.documents.get_mut(uri).unwrap().text="😀x".into();assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":3}},"newText":"y"}]}}))["applied"],true);assert_eq!(e.text(uri),"😀y");}
  64:  Profile::Standard=>unreachable!() }}
  65: }
  66:
  67: #[test]
  68: fn versioned_apply_edit_validates_before_mutating_and_supports_multiple_changes(){let mut e=FakeEditor::new(Profile::Standard);let u="file:///d";e.open(u,"a\nb\n");let result=e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"A"},{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"B"}]}]}));assert_eq!(result["applied"],true);assert_eq!(e.text(u),"A\nB\n");let before=e.text(u).to_owned();let stale=e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[]}]}));assert_eq!(stale["applied"],false);assert!(stale["failureReason"].is_string());assert_eq!(e.text(u),before);}
  69:
  70: #[test]
  71: fn overlapping_and_malformed_edits_reject_atomically_across_documents(){let mut e=FakeEditor::new(Profile::Standard);e.open("file:///a","abcd");e.open("file:///b","wxyz");let result=e.apply_edit(&json!({"changes":{"file:///a":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"newText":"A"},{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"newText":"B"}],"file:///b":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"W"}]}}));assert_eq!(result["applied"],false);assert_eq!(e.text("file:///a"),"abcd");assert_eq!(e.text("file:///b"),"wxyz");}
  72:
  73: #[test]
  74: fn utf16_surrogate_midpoint_is_rejected_and_crlf_lines_are_addressable(){let mut e=FakeEditor::new(Profile::Utf16);e.open("file:///u","😀x\r\ny");assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":2}},"newText":"z"}]}}))["applied"],false);assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"Y"}]}}))["applied"],true);assert_eq!(e.text("file:///u"),"😀x\r\nY");}
  75:
  76: #[test]
  77: fn reconstructed_trace_is_valid_jsonl(){for line in fs::read_to_string("tests/fixtures/lsp_traces/reconstructed-plain.jsonl").unwrap().lines(){let _:Value=serde_json::from_str(line).unwrap();}}
  78:
  79: fn replacement(start:u64,end:u64,text:&str)->Value {json!({"range":{"start":{"line":0,"character":start},"end":{"line":0,"character":end}},"newText":text})}
  80:
  81: #[test]
  82: fn original_snapshot_order_and_atomic_rejection_matrix(){
  83:  let a="file:///a";let b="file:///b";
  84:  let mut e=FakeEditor::new(Profile::Standard);e.open(a,"abcdef");e.open(b,"uvwxyz");
  85:  // Deliberately reversed and length-changing: all offsets use the original.
  86:  assert_eq!(e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":a,"version":1},"edits":[replacement(4,6,"!"),replacement(0,1,"LONG")]},{"textDocument":{"uri":b,"version":1},"edits":[replacement(0,1,"U")]}]}))["applied"],true);
  87:  assert_eq!(e.text(a),"LONGbcd!");assert_eq!(e.text(b),"Uvwxyz");
  88:  let valid=json!({"textDocument":{"uri":a,"version":2},"edits":[replacement(0,1,"X")]});
  89:  let invalids=vec![
  90:   json!({"textDocument":{"uri":b,"version":1},"edits":[]}),
  91:   json!({"textDocument":{"uri":b,"version":"2"},"edits":[]}),
  92:   json!({"textDocument":{"uri":b},"edits":[]}),
  93:   json!({"textDocument":{"uri":b,"version":2},"edits":{}}),
  94:   json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(0,99,"x")]}),
  95:   json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(3,1,"x")]}),
  96:   json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(0,3,"x"),replacement(2,4,"y")]}),
  97:   json!({"textDocument":{"uri":a,"version":2},"edits":[]}),
  98:   json!({"kind":"create","uri":"file:///new"}),
  99:  ];
 100:  for invalid in invalids {let result=e.apply_edit(&json!({"documentChanges":[valid.clone(),invalid]}));assert_eq!(result["applied"],false,"{result}");assert!(result["failureReason"].is_string());assert_eq!(e.text(a),"LONGbcd!");assert_eq!(e.text(b),"Uvwxyz");assert_eq!(e.documents[a].version,2);assert_eq!(e.documents[b].version,2);}
 101:  for edit in [json!({"documentChanges":{}}),json!({"changes":{a:[replacement(0,1,"X")],b:{}}}),json!({"changes":{a:[replacement(0,1,"X")],"file:///missing":[]}}),json!({"documentChanges":[],"changes":{}})] {assert_eq!(e.apply_edit(&edit)["applied"],false);assert_eq!(e.text(a),"LONGbcd!");assert_eq!(e.documents[a].version,2);}
 102: }
 103:
 104: #[test]
 105: fn utf8_utf16_utf32_bounds_use_original_crlf_snapshots(){
 106:  for (encoding,start,end,invalid) in [("utf-8",4,5,Some(1)),("utf-16",2,3,Some(1)),("utf-32",1,2,None)] {
 107:   let mut e=FakeEditor::new(Profile::Standard);e.encoding=encoding;e.open("file:///u","😀x\r\ny");
 108:   if let Some(mid)=invalid {assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[replacement(mid,start,"bad")]}}))["applied"],false);assert_eq!(e.text("file:///u"),"😀x\r\ny");}
 109:   assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[replacement(start,end,"Y")]}}))["applied"],true);assert_eq!(e.text("file:///u"),"😀Y\r\ny");
 110:   assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[replacement(end+1,end+1,"bad")]}}))["applied"],false);
 111:  }
 112: }
 113:
 114: #[test]
 115: fn many_decomposed_ranges_replay_sequentially_in_one_notification(){
 116:  for encoding in ["utf-8","utf-16","utf-32"] {
 117:   let uri="file:///decomposed";let mut e=FakeEditor::new(Profile::DecomposedChanges);e.encoding=encoding;e.open(uri,"😀old\r\ntail");
 118:   let expected=(0..25).map(|i|format!("😀 line {i}\n")).collect::<String>();let notification=e.edit(uri,&expected);
 119:   let changes=notification["params"]["contentChanges"].as_array().unwrap();assert_eq!(changes.len(),26);assert_eq!(notification["params"]["textDocument"]["version"],2);
 120:   let mut replay=FakeEditor::new(Profile::Standard);replay.encoding=encoding;replay.open(uri,"😀old\r\ntail");
 121:   for change in changes {assert!(change["range"].is_object());assert_eq!(replay.apply_edit(&json!({"changes":{uri:[{"range":change["range"],"newText":change["text"]}]}}))["applied"],true);}
 122:   assert_eq!(replay.text(uri),expected);assert_eq!(e.text(uri),expected);
 123:  }
 124: }
 125:
 126: #[test]
 127: fn debounce_releases_no_echo_until_flush_on_actual_transport(){
 128:  let mut client=ClientPump::new(stdio::TestServer::spawn(&[]),Profile::DebouncedDidChange);
 129:  let uri="file:///debounce";let open=client.editor.open(uri,"a");client.send(open);
 130:  let start=client.server.sent.len();
 131:  client.dispatch(&json!({"jsonrpc":"2.0","id":"script/debounce","method":"workspace/applyEdit","params":{"edit":{"changes":{uri:[replacement(0,1,"b")]}}}}));
 132:  assert_eq!(client.server.sent.len(),start+1);assert!(client.editor.has_pending());assert_eq!(client.editor.text(uri),"b");
 133:  for frame in client.editor.flush(){client.send(frame);}
 134:  assert!(!client.editor.has_pending());assert_eq!(client.server.sent.len(),start+2);assert_eq!(client.server.sent[start]["id"],"script/debounce");assert_eq!(client.server.sent[start+1]["method"],"textDocument/didChange");assert_eq!(client.server.sent[start+1]["params"]["textDocument"]["version"],2);
 135:  client.finish();
 136: }
 137:
 138: #[test]
 139: fn shared_dispatch_captures_real_send_order_for_echo_profiles(){for (profile,expected) in [(Profile::Standard,["textDocument/didChange","response"]),(Profile::ReorderEcho,["response","textDocument/didChange"])] {
 140:  let mut client=ClientPump::new(stdio::TestServer::spawn(&[]),profile);
 141:  let open=client.editor.open("file:///order","a");client.send(open);
 142:  // Scripted server request, but actual dispatcher and pipe writes, not production Plain acceptance.
 143:  let request=json!({"jsonrpc":"2.0","id":"script/apply","method":"workspace/applyEdit","params":{"edit":{"documentChanges":[{"textDocument":{"uri":"file:///order","version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"b"}]}]}}});
 144:  let start=client.server.sent.len();client.dispatch(&request);
 145:  let writes=&client.server.sent[start..];assert_eq!(writes.iter().map(|m|m["method"].as_str().unwrap_or("response")).collect::<Vec<_>>(),expected);assert!(writes.iter().any(|m|m["id"]=="script/apply"));assert_eq!(client.editor.text("file:///order"),"b");
 146:  client.dispatch(&json!({"jsonrpc":"2.0","id":"script/show","method":"window/showDocument","params":{"uri":"file:///order"}}));assert_eq!(client.server.sent.last().unwrap()["result"]["success"],true);
 147:  client.dispatch(&json!({"jsonrpc":"2.0","id":"script/unknown","method":"unknown"}));assert_eq!(client.server.sent.last().unwrap()["error"]["code"],-32601);
 148:  client.finish();
 149: }}
 150:
 151: #[test]
 152: fn live_binary_progress_requests_are_serviced_while_command_runs(){
 153:  let (repo,_)=fixtures::RepoFixture::new().commit("init",[("f.txt",fixtures::TEN_LINES)]).modify_standard().build();
 154:  let head=repo.git(&["rev-parse","HEAD"]);
 155:  let mut client=ClientPump::new(stdio::TestServer::spawn(&[]),Profile::Standard);
 156:  let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));let opened=client.until_response(id);let view=opened["result"]["id"].as_str().expect("opened view").to_owned();
 157:  let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"status.stage-all"}}));let response=client.until_response(id);assert!(response.get("result").is_some(),"{response}");
 158:  assert!(repo.git(&["diff","--cached","--name-only"]).contains("f.txt"));
 159:  // Cancel only when the actual commit editor is waiting. Cancelling on begin
 160:  // races the setup rev-parse, whose killed process returns an RPC error.
 161:  client.cancel_on_edit=true;
 162:  let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"commit.create"}}));assert!(client.until_response(id).get("result").is_some());
 163:  client.until(|c| !c.cancel_on_edit && c.server.notifications.iter().filter(|m|m["method"]=="$/progress"&&m["params"]["value"]["kind"]=="end").count()==2);
 164:  let requests:Vec<_>=client.editor.outbound.iter().filter(|m|m["method"]=="window/workDoneProgress/create").collect();assert_eq!(requests.len(),2);
 165:  assert_ne!(requests[0]["id"],requests[1]["id"]);
 166:  for request in requests {assert!(client.server.sent.iter().any(|m|m["id"]==request["id"]&&m.get("result")==Some(&Value::Null)));let token=&request["params"]["token"];let phases:Vec<_>=client.server.notifications.iter().filter(|m|m["method"]=="$/progress"&&m["params"]["token"]==*token).map(|m|m["params"]["value"]["kind"].as_str().unwrap()).collect();assert_eq!(phases.first(),Some(&"begin"));assert_eq!(phases.last(),Some(&"end"));assert!(phases[1..phases.len()-1].iter().all(|p|*p=="report"));}
 167:  assert!(client.server.sent.iter().any(|m|m["method"]=="window/workDoneProgress/cancel"));assert_eq!(repo.git(&["rev-parse","HEAD"]),head);
 168:  client.finish();
 169: }
```

#### `Makefile` — complete (45 lines)

```text
   1: .PHONY: build test test-rust test-lua test-file ui-unit ui-test ui-tour color-spike vscode-test
   2:
   3: .SILENT:
   4: MAKEFLAGS += --no-print-directory
   5: export CARGO_TERM_COLOR=never
   6: export CARGO_TERM_PROGRESS_WHEN=never
   7: LOG_TAIL ?= 150
   8:
   9: define run
  10: @if [ -n "$(V)" ]; then $(2); else mkdir -p build/test-logs || exit $$?; log=$$(mktemp build/test-logs/$(1).XXXXXX) || exit $$?; if $(2) >"$$log" 2>&1; then status=0; printf 'ok: $(1)\n'; rm -f "$$log"; else status=$$?; tail -n $(LOG_TAIL) "$$log"; printf 'FAILED: $(1) (full log: %s)\n' "$$log"; fi; exit $$status; fi
  11: endef
  12:
  13: build:
  14: 	$(call run,build,cargo build -q)
  15:
  16: # `make test` covers the Rust, Neovim and VSCode suites. The real-UI Python
  17: # suites are separate: `make ui-unit` (harness) and `make ui-test`/`make ui-tour`
  18: # (scripted Neovim screens) must be run when touching the UI harness or
  19: # user-visible rendering.
  20: test: build test-rust test-lua vscode-test
  21:
  22: vscode-test: build
  23: 	$(call run,vscode,npm --silent --prefix clients/vscode test)
  24: 	$(call run,keybindings,node clients/vscode/scripts/gen-keybindings.mjs --check)
  25:
  26: test-rust:
  27: 	$(call run,rust,cargo test -q $(FILTER))
  28:
  29: test-lua: build
  30: 	$(call run,lua,nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()")
  31:
  32: test-file: build
  33: 	$(call run,lua,nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')")
  34:
  35: ui-unit:
  36: 	$(call run,ui-unit,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/ui -p 'test_*.py')
  37:
  38: ui-test:
  39: 	$(call run,ui-test,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/scenarios/*.scn tests/ui/smoke.scn)
  40:
  41: ui-tour:
  42: 	$(call run,ui-tour,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/tour.scn tests/ui/tour-clean.scn tests/ui/tour-error.scn --markdown build/ui/tour.md)
  43:
  44: color-spike: build
  45: 	$(call run,color-spike,nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua)
```

#### `tests/ui/plain_init.lua` — complete (32 lines)

```text
   1: -- Standalone standard-LSP init for the Plain native smoke. No project Lua modules.
   2: vim.opt.swapfile = false
   3: vim.opt.undofile = false
   4: vim.opt.shortmess:append('I')
   5: vim.g.plain_smoke_rich_notifications = 0
   6: vim.lsp.log.set_level('trace')
   7: vim.lsp.handlers['view/render'] = function()
   8:   vim.g.plain_smoke_rich_notifications = vim.g.plain_smoke_rich_notifications + 1
   9: end
  10:
  11: local repo = vim.env.PLAIN_REPO
  12: local server = vim.env.PLAIN_SERVER
  13: assert(repo and server)
  14:
  15: vim.api.nvim_create_autocmd('VimEnter', {
  16:   once = true,
  17:   callback = function()
  18:     local capabilities = vim.lsp.protocol.make_client_capabilities()
  19:     -- Stock Neovim executes server commands but omits this optional capability
  20:     -- object by default. Declare static support, not dynamic registration.
  21:     capabilities.workspace.executeCommand = { dynamicRegistration = false }
  22:     local client_id = vim.lsp.start({
  23:       name = 'magit-lsp-plain-smoke',
  24:       cmd = { server },
  25:       root_dir = repo,
  26:       capabilities = capabilities,
  27:     })
  28:     assert(client_id, 'vim.lsp.start did not create a client')
  29:     vim.g.plain_smoke_client_id = client_id
  30:     vim.lsp.buf_attach_client(0, client_id)
  31:   end,
  32: })
```

#### `ui-test-plan.md` A.5 — verbatim complete section (source lines 320-353; next same-level heading at 357)

```text
 320: ### A.5 Scenario DSL
 321:
 322: Plain text, one statement per line, `#` comments. Header before `---`:
 323:
 324: ```
 325: fixture: modified-two-files
 326: size: 100x30            # runner also runs 80x24 unless "size-only: true"
 327: profile: vim            # runner also runs "standard" unless "profile-only: true"
 328: xfail: bug-2            # optional; see Rules
 329: env: MAGIT_LSP_PLACEMENT=vsplit   # optional extra Neovim environment
 330: ---
 331: keys: m ?               # space-separated keys, each sent separately
 332: goto "+line3 CHANGED"   # move the cursor to the first line containing the text
 333: wait                    # until idle
 334: expect screen contains "Transient and dwim commands"
 335: expect screen not-contains "Committing"
 336: expect screen matches /Unstaged changes \(\d+\)/
 337: expect windows.floating == 1
 338: expect view_stack.menus == 1
 339: expect pending == "-"
 340: expect no_message /W10|readonly/
 341: expect style "(--all)" dim              # dim | bold | coloured | plain
 342: expect cursor on "Unstaged changes"
 343: expect latency("?") <= 100ms            # key-to-idle for the last key named
 344: snapshot: name                          # writes build/ui/<scenario>/<name>.screen and .ansi
 345: ```
 346:
 347: `style` predicates use the ANSI decoder on the run containing the quoted text. `latency` uses the trace. A failed `expect` prints the expectation, the actual value, and the full current screen, then continues to the next scenario.
 348:
 349: `goto "<text>"` moves the cursor to the first character of the first buffer line containing the quoted text (a literal match, no regex). It fails when the text is absent. It is an RPC call rather than a key press, so it neither appears in the trace nor contributes to a `latency` measurement; scenarios use it to name a target instead of counting motion keys.
 350:
 351: `sh: <command>` runs a POSIX shell command in the fixture repo with the deterministic Git environment and `/dev/null` stdin, between key batches. Scenarios use it to change the repository outside Neovim (the `auto-refresh` watcher test modifies a tracked file this way); any direct Neovim RPC command must include `--headless`. A failed, timed-out or unlaunchable shell step is reported with its statement number, command and bounded detail as a failed combination; remaining size/profile combinations still run, and that run is always stopped.
 352:
 353: `latency` always reports the measured key-to-idle time. Budgets are multiplied by `UI_BUDGET_SCALE`, and an over-budget measurement only fails the run when `UI_STRICT_LATENCY=1`; otherwise it is informational.
```

#### `tests/common/session.rs` pump/dispatch signatures and implementations

Signatures and function index are source-verbatim; implementation excerpts
show the actual production entry points and render/focus bookkeeping.

```text
--- source lines 1-9 ---
   1: //! Shared in-process [`TestSession`] harness.
   2: //!
   3: //! The harness owns one real [`Session`], the focused view, the cursor and the
   4: //! scripted prompt UI. It always drives the production `view/key` and
   5: //! `view/invoke` paths, remembers the last render version it applied and sends
   6: //! that version with every key request, so tests never hard-code a version.
   7: //!
   8: //! Cursor targets are resolved through the server tree (`line_nodes`, `path`,
   9: //! `marker`, `text`, `copy`); rendered text is never parsed to find structure.
--- source lines 56-58 ---
  56: /// One in-process session driven like a client: key routing, invokes, acks and
  57: /// prompt answers all go through the real [`Session`] entry points.
  58: pub struct TestSession {
--- source lines 76-103 ---
  76: impl TestSession {
  77:     /// A session for `repo` with the profile's keymaps and no open view.
  78:     pub fn new(repo: impl Into<String>, profile: Profile) -> Self {
  79:         Self::with_config(
  80:             repo,
  81:             Config {
  82:                 profile,
  83:                 ..Default::default()
  84:             },
  85:         )
  86:     }
  87:
  88:     /// A session for `repo` with an explicit configuration.
  89:     pub fn with_config(repo: impl Into<String>, config: Config) -> Self {
  90:         let mut session = Session::new(repo);
  91:         session.set_keymaps(Arc::new(
  92:             keymap::validate(&config).expect("resolve keymaps"),
  93:         ));
  94:         Self {
  95:             session,
  96:             config,
  97:             view: "v1".to_string(),
  98:             cursor: Cursor { line: 0, column: 0 },
  99:             versions: BTreeMap::new(),
 100:             ui: ScriptedUi::new(),
 101:             key_version: 0,
 102:         }
 103:     }
--- source lines 229-247 ---
 229:     pub fn ack(&mut self) {
 230:         self.ack_view(&self.view.clone());
 231:     }
 232:
 233:     /// Acknowledge the last applied render of `view_id`.
 234:     pub fn ack_view(&mut self, view_id: &str) {
 235:         let version = self.version_of(view_id);
 236:         self.session
 237:             .ack(view_id, version)
 238:             .unwrap_or_else(|error| panic!("ack {view_id} at version {version}: {error}"));
 239:         assert_eq!(
 240:             self.acked_version_of(view_id),
 241:             Some(version),
 242:             "ack of {view_id} at version {version} did not take"
 243:         );
 244:     }
 245:
 246:     /// The cursor used by [`Self::press`] and [`Self::invoke_here`].
 247:     pub fn cursor(&self) -> Cursor {
--- source lines 448-483 ---
 448:     pub fn press(&mut self, keys: &str) -> InvokeResult {
 449:         let steps: Vec<&str> = keys.split_whitespace().collect();
 450:         assert!(!steps.is_empty(), "press needs at least one key step");
 451:         let mut last = None;
 452:         for step in steps {
 453:             last = Some(self.press_full(step, EditorMode::Normal, self.cursor, None));
 454:         }
 455:         last.expect("at least one step")
 456:     }
 457:
 458:     /// Press `key` at `cursor` with the given editor mode and selection.
 459:     pub fn press_full(
 460:         &mut self,
 461:         key: &str,
 462:         mode: EditorMode,
 463:         cursor: Cursor,
 464:         selection: Option<Selection>,
 465:     ) -> InvokeResult {
 466:         let view = self.view.clone();
 467:         let version = self.version_of(&view);
 468:         self.key_version = version;
 469:         let result = self
 470:             .session
 471:             .key(&ViewKeyParams {
 472:                 view_id: view,
 473:                 version,
 474:                 key: key.to_string(),
 475:                 editor_mode: mode,
 476:                 cursor,
 477:                 selection,
 478:             })
 479:             .expect("view/key");
 480:         self.track_result(&result);
 481:         result
 482:     }
 483:
--- source lines 543-582 ---
 543:     pub fn invoke(&mut self, command: &str) -> InvokeResult {
 544:         self.invoke_full(command, None, None, None)
 545:     }
 546:
 547:     /// Invoke `command` with JSON arguments and no cursor.
 548:     pub fn invoke_args(&mut self, command: &str, args: Value) -> InvokeResult {
 549:         self.invoke_full(command, None, None, Some(args))
 550:     }
 551:
 552:     /// Invoke `command` at `cursor`.
 553:     pub fn invoke_at(&mut self, command: &str, cursor: Cursor) -> InvokeResult {
 554:         self.invoke_full(command, Some(cursor), None, None)
 555:     }
 556:
 557:     /// Invoke `command` at `cursor` with JSON arguments.
 558:     pub fn invoke_at_args(
 559:         &mut self,
 560:         command: &str,
 561:         cursor: Option<Cursor>,
 562:         args: Value,
 563:     ) -> InvokeResult {
 564:         self.invoke_full(command, cursor, None, Some(args))
 565:     }
 566:
 567:     /// Invoke `command` at the stored cursor.
 568:     pub fn invoke_here(&mut self, command: &str) -> InvokeResult {
 569:         self.invoke_full(command, Some(self.cursor), None, None)
 570:     }
 571:
 572:     /// Invoke `command` with a cursor and a visual selection.
 573:     pub fn invoke_selection(
 574:         &mut self,
 575:         command: &str,
 576:         cursor: Option<Cursor>,
 577:         selection: Option<Selection>,
 578:     ) -> InvokeResult {
 579:         self.invoke_full(command, cursor, selection, None)
 580:     }
 581:
 582:     /// Invoke `command`, answering any prompt from the harness's [`ScriptedUi`].
--- source lines 619-650 ---
 619:     pub fn invoke_full(
 620:         &mut self,
 621:         command: &str,
 622:         cursor: Option<Cursor>,
 623:         selection: Option<Selection>,
 624:         args: Option<Value>,
 625:     ) -> InvokeResult {
 626:         let version = self.version();
 627:         self.invoke_at_version(command, version, cursor, selection, args)
 628:     }
 629:
 630:     /// Invoke using coordinates from an explicitly observed render version.
 631:     pub fn invoke_at_version(
 632:         &mut self,
 633:         command: &str,
 634:         version: u64,
 635:         cursor: Option<Cursor>,
 636:         selection: Option<Selection>,
 637:         args: Option<Value>,
 638:     ) -> InvokeResult {
 639:         let view = self.view.clone();
 640:         let result = self
 641:             .session
 642:             .invoke(&ViewInvokeParams {
 643:                 view_id: view,
 644:                 command: command.to_string(),
 645:                 version: Some(version),
 646:                 args,
 647:                 cursor,
 648:                 selection,
 649:             })
 650:             .expect("invoke");
--- source lines 708-735 ---
 708:     /// Record a render as applied and focus a newly pushed view.
 709:     fn track_result(&mut self, result: &InvokeResult) {
 710:         if let Some(render) = &result.render {
 711:             self.apply(render);
 712:         }
 713:         for (render, _keymap) in &result.opened {
 714:             self.apply(render);
 715:         }
 716:         let focused = result
 717:             .opened
 718:             .last()
 719:             .map(|(render, _)| render.view_id.clone())
 720:             .or_else(|| {
 721:                 result.effects.iter().rev().find_map(|effect| match effect {
 722:                     Effect::FocusView { view_id } => Some(view_id.clone()),
 723:                     _ => None,
 724:                 })
 725:             });
 726:         if let Some(view_id) = focused {
 727:             self.view = view_id;
 728:         }
 729:     }
 730:
 731:     /// Record the render version applied for its view.
 732:     fn apply(&mut self, render: &ViewRender) {
 733:         self.versions.insert(render.view_id.clone(), render.version);
 734:     }
 735: }
```

Function/signature index from `grep -nE '^    pub fn (press|invoke|ack|open_view|open_status|set_mode)|^    fn (press|invoke|ack|open_view|render)' tests/common/session.rs`: constructors `new` 78, `with_config` 89; open status/view 110, 115, 126, 132, 150; ack 229, 234; `press`, `press_full`, `press_at_version`, `press_visual`, `press_ui` 448, 459, 485, 504, 520; invoke family 543-631; set_mode 656; `track_result` 709; `apply` 732; second impl `new_at` 740. The full `ClientPump` implementation is already included above in the complete `tests/plain_harness.rs` block.

#### Captured command outputs

Truecolor experiment output (actual stdout from owned-session capture; repr and
hex include the terminal line padding):

```text
exit 0 stderr
repr b'\\x1b[38;2;18;52;86mCOLOR\\x1b[39m\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n'
hex 1b5b33383b323b31383b35323b38366d434f4c4f521b5b33396d0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a
has_rgb_sgr True
```

Focused repository text inventory (only names/terms; no image bytes read or opened). Excludes `.git`, generated `build`, `node_modules`, `target`, and this report itself to avoid generated artifact and recursive self-matches:

```text
$ grep -RInE 'png|screenshot|jpg|video|trace\.zip' --exclude-dir=.git --exclude-dir=build --exclude-dir=node_modules --exclude-dir=target --exclude='harness-reassessment-inputs.md' . 2>/dev/null
./REVIEW_REPORT.md:283:  - **Final native/full gate (2026-10-27):** The single final `npm --prefix clients/vscode run test:e2e && make test && git diff --check` exited 0. The default full native suite reached A06 without bypassing Enter/folding assertions. Inspected `/tmp/opencode/vscode-review-P45Bx8/a06-menu-before-quit.png`, `a06-quit.png`, and `a06-reopened.png`: Commit QuickPick painted before quit, dismissed over the real tracked file after quit with the unrelated Status tab retained, and reopened Status visibly rendered alongside that tab. `a06-evidence.json` confirms removed views `v1` and `v1:menu.commit`, preserved root `v2`, reopened root `v3`, no recursive `view/close`, no real-file Magit key dispatch, and physical `g` dispatching `common.refresh` on `v3`. `make test` reported `ok: build`, `rust`, `lua`, `vscode`, and `keybindings`. Inspected invocation producers: the suite already supplies the fourth `info` argument; both production callers (`commands.js`, `edits.js`) supply `info.cursor`, so no fallback or producer change was needed. No editor/runner for this isolated directory remained; other-session test processes were observed and left untouched.
./REVIEW_REPORT.md:284:  - **Exact ticket gate (2026-10-27):** `npm --prefix clients/vscode run test:e2e && make vscode-test && npm --prefix clients/vscode run test:integration && make test && git diff --check` exited 0. Inspected the final default-suite A06 screenshots and `a06-evidence.json` at `/tmp/opencode/vscode-review-C36z2h`; VSCode test and Node integration both passed, and `make test` reported build/Rust/Lua/VSCode/keybindings green. No owned editor/server processes remained.
./REVIEW_REPORT.md:305:  - **Completion evidence (2026-10-27):** Removed the raw-key confirm/cancel policy; edit context now uses `effect.scope`, and `currentEdit()` only resolves the focused editor document. The real-server Node integration drives commit creation, generated `common.confirm`/`common.cancel` dispatcher bindings, and verifies confirm creates the requested commit while cancel does not. Native VSCode A07 uses a dirty real bridge document to prove save-before-confirm; it separately exercises generated confirm/cancel IDs, native close cancellation, and `common.refresh` reaching the server while another edit tab remains open. Inspected `/tmp/opencode/vscode-review-3IYDyR/a07-edit-{before-confirm,confirmed,cancelled,closed}.png` and `a07-evidence.json`; the before screenshot shows the dirty Git Commit Message editor, confirm returns to status with the committed message, and cancel/close return to status without a new commit. Exact final gate `make vscode-test && npm --prefix clients/vscode run test:integration && npm --prefix clients/vscode run test:e2e && make test && git diff --check` exited 0; `make test` reported build/Rust/Lua/VSCode/keybindings green. Five earlier gate attempts failed during iteration (three staged-fixture checks and two fake-editor focus timeouts); the final run passed. Post-run process inspection found no owned VSCode/e2e/server process remaining; no process was killed.
./REVIEW_REPORT.md:614:  - **Acceptance evidence (2026-10-27):** Exact final gate `make test && make vscode-test && scripts/ui run tests/ui/scenarios/text-mode-roundtrip.scn && git diff --check` passed. `/tmp/opencode/vscode-review-04pmBr/a29-evidence.json` records metadata-only same-version entry, invoke/key/menu/setMode coverage, native selection/copy/navigation, queued refresh selection stability and applied replacement on exit, and capture=true with zero suppressed requests. Inspected real UI screenshots `a29-before.png`, `a29-during.png`, `a29-queued.png`, `a29-after.png`, and `a29-capture.png`; full gate retains Neovim semantics. A11 remains deferred as previously recorded.
./REVIEW_REPORT.md:659:  - **Native acceptance (2026-10-27):** Exact gate `cargo build && cargo test --test menu --test menus && make vscode-test && MAGIT_VSCODE_A33_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check` exited 0 (30 Rust menu tests plus VSCode/unit/keybinding gates and native e2e). The isolated fixture configures `magit-lsp.profile = "vim"` before opening. Real `open.commit` / dispatch rows are selected with native arrow keys and Return, never typed filters or synthetic metadata. Exact view/key identities and sequences prove `-a` → `['-', 'a']` toggles the server switch and rendered `--all · off` → `on` row; `ctrl+t` → one request changes server-rendered mode to `text`; `g l` → `['g', 'l']` opens the native Log menu. Inspected all six readable `a33-{flag,modifier,spaced}-{before,after}.png` screenshots: Commit off/on, Dispatch modifier selection/list reset (text mode recorded in JSON), and Dispatch Log selection → Log menu. Retained screenshots, server metadata, exact requests, rendered lines and modes in `/tmp/opencode/vscode-review-kiTL7h/a33-evidence.json`. Earlier wrong-menu and initial-render/selection races are resolved. Only owned host/server processes were stopped; final process check found none remaining.
./REVIEW_REPORT.md:674:  - **Native completion (2026-10-27):** The preceding progress paragraph is retained verbatim as historical evidence, relocated from A02. A34 is now complete. `cargo build && MAGIT_VSCODE_A34_ONLY=1 npm --prefix clients/vscode run test:e2e && make test && git diff --check` passed. The isolated native branch runs before unrelated widgets and fails on missing editor/display/server or `a34-evidence.json`; A29 textmode and A33 activation branches are unchanged. Sixteen real VSCode `Selection` cases use registered `magit.key` and `magit.invoke`, the rebuilt server and real Git without mutation stubs: addition/deletion × stage/unstage × colzero/nonzero ends × dispatcher, with forward/reverse ranges, context plus adjacent changed rows and independently asserted active cursors. Server `documentSymbol` hunk metadata locates fixture rows; exact fresh requests (including version, inclusive selection and cursor) and base64 HEAD/index/worktree/expected-index bytes are retained in `/tmp/opencode/vscode-review-M0B3HF/a34-evidence.json`. FIRST alone or FIRST+SECOND changes, while SECOND/GUARD or GUARD respectively remains untouched; every worktree stays byte-identical. Thirty-two readable before/after screenshots are retained there; inspected `a34-add-stage-colzero-key-{before,after}.png` shows FIRST alone staged with SECOND/GUARD still unstaged, and `a34-delete-unstage-nonzero-invoke-{before,after}.png` shows FIRST/SECOND deletions unstaged with GUARD deletion still staged. Native document and painted Monaco rows are polled before screenshots, including both resulting changed sections, preventing stale pre-mutation screenshots. Full regression output: build/rust/lua/vscode/keybindings all green. Owned host cleanup and extension deactivation completed; process inspection found no remaining magit-lsp or owned isolated host processes. Native cases complement, rather than repeat, the 72 shape/240 real-Git automated cases above.
./magit-lsp-plain-client-tasks.md:6:never use vision, inspect/render images, invoke image/screenshot tools, or capture
./magit-lsp-plain-client-tasks.md:7:screenshots. Native UI verification remains real, using text/structured state
./magit-lsp-plain-client-tasks.md:133:pre-action screenshots retained and inspected. Core fallback and dirty choices
./magit-lsp-plain-client-tasks.md:252:`real-file-native-4-wrong-version-retained.png`, `real-file-native-5-unsafe-{rejected,recovered}.png`,
./magit-lsp-plain-client-tasks.md:253:`real-file-native-6-stale-probes-retained.png` and commit-result screens.
./README.md:8:[native screenshot capture/manual fallback](docs/SCREENSHOTS.md), and the
./TASKS.md:612:    trace}.json` and `real-file-native-3-discard-dialog.png` retain production
./TASKS.md:615:    never unsaved text. Additional inspected screenshots in the passing
./TASKS.md:687:    and `real-file-native-trace.json`, plus `real-file-native-2-cancel-dialog.png`
./TASKS.md:688:    and `real-file-native-3-committed.png`, verify actual native Save/Don't Save/
./TASKS.md:1403:circumstances use vision, inspect/render images, invoke image/screenshot tools,
./TASKS.md:1404:or capture screenshots. Native UI verification remains real, but uses text and
./TASKS.md:1426:#### NO-VISION-TESTS — Remove executable screenshot paths
./TASKS.md:1428:- [x] **[P0] Remove image/screenshot test paths and replace checkpoints with substantive text evidence.** Preserve real native interactions and all nonvisual assertions, and retain text snapshots.
./TASKS.md:1431:  - **Done when:** all screenshot capture and image-snapshot executable paths are removed; meaningful native checkpoints use text/structured state (accessibility/DOM text, JSON, wire/API and Git state) while preserving interaction and nonvisual assertions. No image-based acceptance or future screenshot inspection remains.
./TASKS.md:1472:  - [x] **Native reset-author acceptance (2026-10-29):** Reused Rust acceptance from `9002cbd`; typed `-R` in Neovim Reword (both profiles, both sizes) and VSCode Amend/clean Extend/staged Reword. Exact before/after author/date, message/tree/index/worktree, parents/count and applicable committer-date checks pass; real saved native closure is inspected. `cargo build --release && scripts/ui run tests/ui/scenarios/commit-head-operations.scn && MAGIT_VSCODE_BIN=/home/<user>/.nix-profile/bin/codium MAGIT_VSCODE_HEAD_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check` exited 0. Retained Neovim `reset-author-{option,saved,result}.screen` and `reset-author-evidence.json` under all four `build/ui/commit-head-operations/{standard,vim}-{100x30,80x24}/` variants; VSCode `/tmp/opencode/vscode-review-nmM2OM/` records eight original plus three reset-author cases and 39 named screenshots. Inspected screens/JSON and verified owned cleanup. Failed fixture/assertion/schema attempts are diagnosed in `docs/evidence/commit-head-operations.md`. R-4.1-head-edit's stated typed-operation, guard, Rust state, and both-client native criteria are complete; broad gates remain failed as documented.
./TASKS.md:1533:  - **Constraints:** Do not weaken `parse_file_diff()`'s refusal to treat a multi-section type change as one text patch. Do not reconstruct the original identity from fresh Git state at invocation or prompt creation, hash the whole repository per row, parse rendered text, change bulk-command semantics, or narrow the documented whole-file contract to expanded text files. Preserve inherited release edits; use no images or screenshots.
./TASKS.md:1541:  - **Constraints:** Do not bypass stale-state validation for all untracked files, substitute the untracked section's bulk command for a file action, change unrelated dispatch semantics, add a client-side workaround, or modify inherited release edits. No images or screenshots.
./TASKS.md:1572:  - **Constraints:** Do not remove ordinal disambiguation, introduce hunk-index addressing, broaden bulk restrictions, change whole-file behavior, parse rendered text, or modify inherited release edits. No images or screenshots.
./TASKS.md:1579:  - **Constraints:** Keep this a focused backend/tree/dispatcher correction, not a generic VCS abstraction or token registry. Do not strip meaningful headers from execution, change deliberate bulk semantics, weaken whole-file validation, or introduce full-file identity checks that invalidate harmless unique-hunk shifts. Preserve inherited edits; no images or screenshots.
./TASKS.md:1751:  - **Inspected native evidence:** `/tmp/opencode/vscode-review-uDwXKm/real-file-native-evidence.json` and `real-file-native-trace.json`; screenshots `real-file-native-{save-alone,two-groups,nonlast-close,cancel-retained}.png`, `real-file-native-2-{cancel,save}-dialog.png`, `real-file-native-3-discard-dialog.png`, and `real-file-native-3-committed.png`. Save alone, switching/focus, language change and nonlast two-group close send no confirmation/change no HEAD. Cancel retains the dirty tab/run. Saved last-close commits exact subject/body `native saved message\n\nexact body` (HEAD `14b8cf0d72162ded52943209cc9e841138dc7b3a`); native Save commits `native Save new bytes\n\nnew body` (`01f0e865e4080264e53f64b04177d8167a929e07`); Don't Save commits `native PREVIOUS saved bytes\n\nold body` (`48eb44fbaf3427d68ab696a7c53b5382dec120c4`) despite verified unsaved full-sync version 3. Each run issues exactly one saved-file confirmation/commit, and subsequent runs have fresh Git text and distinct run IDs. Owned native host/server processes were absent after cleanup. No synthetic didClose, observer-spike substitution or shutdown confirmation; history removal is a separate pending slice and parent R-EDIT-REAL remains unchecked.
./TASKS.md:1764:  - [x] **VSCode native HEAD-only slice (2026-10-29):** `MAGIT_VSCODE_BIN=/home/<user>/.nix-profile/bin/codium MAGIT_VSCODE_HEAD_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check` exited 0. Normal production host, real typed commit-menu options/actions and saved native last-tab closures cover staged/clean unchanged Amend, exact edited Reword with nonempty index/tree/date retained, no-editor Extend with exact message/date, tracked-only `--all`, clean guard/allow-empty and exact-state explicit abort. All replacements retain original parents/count, not a new descendant. Machine-readable state/requests and 26 screenshots are retained in `/tmp/opencode/vscode-review-tAX9dE/`; inspected native evidence and harness-only failure diagnoses are detailed in `docs/evidence/commit-head-operations.md`. No production changes, synthetic didClose/dialogs, added sleeps or deadline inflation. Owned host/server processes stopped; R-4.1-head-edit stated client criteria complete; broad gates remain failed as documented.
./TASKS.md:1938:  - [x] **R-EDIT-REAL multi-root native isolation slice (2026-10-29).** EDIT_ONLY's normal live Codium host owns two canonical repository sessions (`v1`/`v2`) and simultaneous distinct runIds/URIs/messages. Both-direction run/URI swaps and independent-connection owner confirmations are labelled semantic protocol probes (synchronized and saved-file), rejected with exact per-root HEAD/tree/index/worktree/count/message/status and pending disk/live text unchanged. Real saved last-tab close commits A exactly once (count 7→8), while B stays open/pending with count 1 and unchanged exact state. Production `magit.cancelEdit` aborts B, preserves both roots and B's file, then actual reopen/late tab close sends no new confirmation or commit. Inspected all four `real-file-native-two-repository-*.png` and before/after/request evidence in `/tmp/opencode/vscode-review-S81D41/real-file-native-{trace,evidence}.json`; B's staged delta remains visible after Abort (the existing Git-abort failure toast is not a B commit). **Setup root cause:** explicit second-root `view/open`/production invoke already routed correctly; clean Create was refused before any helper/draft. Stage independent deltas in both roots, not longer deadlines. A second workbench LanguageClient attempt collided with registered `magit.command`; independent-owner probes instead use a labelled stdio connection, shut down and awaited in cleanup. No production/protocol change, reload/deactivation, fabricated native event, sleep or increased deadline. No magit-lsp helpers remained. Same-URI/late/duplicate semantic probes and native nonlast/last-tab and unsafe-read recovery count for their finite criteria; duplicate native events and universal platform guarantees are not claimed. This slice supersedes the parent's old multi-root evidence gap only; parent and broad gates remain unchecked; retained ring policy is unchanged. Verification exited 0: `MAGIT_VSCODE_BIN=/home/<user>/.nix-profile/bin/codium MAGIT_VSCODE_EDIT_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check`.
./TASKS.md:1982:  - **Evidence:** `npm --prefix clients/vscode run test:integration` and `npm --prefix clients/vscode run test:e2e` pass against the real server; real native `r-edit-5-*.png` screenshots and `r-edit-5-evidence.json` / `r-edit-5-disconnect.json` inspected; save-stays-editing, confirm, abort, dirty Save/Discard/Cancel, ambiguous tab close, progress cancel, version-error recovery, history recall and return-to-current-draft covered; owned UI stopped; `git diff --check` clean (see §3.10).
./TASKS.md:2203:- [ ] **R-REL-2 — Client install and screenshots**
./TASKS.md:2206:  discovery. Add cheap deterministic CI screenshot capture for both real
./TASKS.md:2215:  screenshot assets or manual fallback are usable; client spec is linked and
./TASKS.md:2218:  screenshot job checked for both harnesses; spec link/consumption review.
./TASKS.md:2222:    discovery for both clients, and usable screenshot capture/manual-fallback
./TASKS.md:2225:    clean-machine and screenshot evidence. Link `docs/CLIENT_SPEC.md` and welcome
./TASKS.md:2229:    server setup and existing native screenshot/manual-capture paths. All 13
./TASKS.md:2236:    release-ready menu screenshot. The lazy.nvim manager, desktop-terminal PNG
./TASKS.md:2244:  quickstart, screenshots, links and concise feature-focused roadmap. Publish
./clients/vscode/NOTES.md:9:The screenshot descriptions in the dated sections below are historical records
./clients/vscode/NOTES.md:34:`r-vs-4-theme-dark.png`, `r-vs-4-theme-light.png` and
./clients/vscode/NOTES.md:35:`r-vs-4-theme-dark-restored.png` are actual renderer captures through CDP
./clients/vscode/NOTES.md:46:| Screenshots | `r-vs-4-theme-dark.png`, `r-vs-4-theme-light.png`, `r-vs-4-theme-dark-restored.png` show the same `Status` document with painted headings, additions and deletions; dark and light screenshots visibly differ in chrome and token colours, and the restored screenshot matches the first. |
./clients/vscode/NOTES.md:94:| Arbitrary ID and opaque arguments forwarded unchanged | `magit.invoke { command: "common.hide-section", args: { recursive: true } }` produced `view/invoke` `{ viewId: "v1", command: "common.hide-section", cursor: { line: 4, column: 0 }, args: { recursive: true } }`. The server-rendered text visibly collapsed the targeted section (`r-vs-3-generic-dispatch.png`). Dispatching `common.show-section` with the same `{ recursive: true }` restored the exact previous server text (`r-vs-3-generic-restored.png`). The arguments were opaque: nothing was added, removed or rewritten. |
./clients/vscode/NOTES.md:95:| Another action through the dispatcher visibly takes effect | `magit.invoke { command: "status.stage-all" }` sent that canonical ID and moved the file: `git diff --cached --name-only` was empty before and `tracked.txt` after, and the status render visibly changed to `Staged changes (1)` / `Unstaged changes (0)` (`r-vs-3-generic-action.png`). `status.unstage-all` through the same dispatcher restored an empty index and `Staged changes (0)` (`repositoryRestored: true`). |
./clients/vscode/NOTES.md:96:| Default fold shortcut in active status | Physical `Tab` (the server-owned `common.toggle-section` default) sent exactly two `view/invoke` requests `{ viewId: "v1", command: "common.toggle-section", cursor: { line: 4, column: 0 } }`. The screen visibly folded the `Unstaged changes (1)` section (`r-vs-3-fold.png`) and restored it on the second press. Two additional requests came from the mirrored `ctrl+alt+]` binding, giving four toggles in total. |
./clients/vscode/NOTES.md:97:| Default refresh shortcut in active status | Physical `g` sent exactly one `view/invoke` `{ viewId: "v1", command: "common.refresh", cursor: { line: 4, column: 0 } }`; the server reread the worktree and rendered the new `+after-return` line in the same visible, server-owned view (`06-status-refresh.png`). The document text equalled the received server render and the URI/tab were unchanged. |
./clients/vscode/NOTES.md:99:All of these screenshots were opened and inspected: `00-status-expanded.png`,
./clients/vscode/NOTES.md:100:`r-vs-3-fold.png`, `r-vs-3-generic-dispatch.png`,
./clients/vscode/NOTES.md:101:`r-vs-3-generic-restored.png`, `r-vs-3-generic-action.png`,
./clients/vscode/NOTES.md:102:`06-status-refresh.png`, `07-magit-quickpick.png`,
./clients/vscode/NOTES.md:103:`08-f5-before-dismissal.png`, `08-f5-probe.png`, `08-f5-control-g.png`, and the
./clients/vscode/NOTES.md:104:run's `r-vs-2-*.png` files. Text and tree ownership stayed with the server:
./clients/vscode/NOTES.md:117:step, and the F5 diagnostic screenshots show no `+f5-*` content.
./clients/vscode/NOTES.md:144:traces/screenshots, a bounded retry for focusing the existing find-widget setup,
./clients/vscode/NOTES.md:168:- `r-vs-2-first-open.png` — screen captured before any reopen/expand/action.
./clients/vscode/NOTES.md:169:- `r-vs-2-reopen.png` — screen after one sequential and two concurrent opens.
./clients/vscode/NOTES.md:170:- `r-vs-2-refresh.png` — screen after the `g` refresh.
./clients/vscode/NOTES.md:179:| First open from a fresh window | `view/open` returned descriptor `id=v1`, `kind=status`, `title=Status`, server URI `magit://status/tmp/opencode/vscode-review-K6aS1m/repo`. The VSCode document is `magit://v1/Status`; its text exactly equalled the received full `view/render` text (version 1, 12 lines), including `Head: main`, `Unstaged changes (1)`, `M tracked.txt`, `-original`, `+replacement` and `Untracked files (1)`. The tab label is `Status`; exactly one status tab existed; the editor was visible and its visible ranges covered line 0. `r-vs-2-first-open.png`. |
./clients/vscode/NOTES.md:180:| Repeat/concurrent opens | One sequential `magit.open` plus two concurrent calls produced **no** additional `view/open` request (count stayed 1), one `Status` tab, the same `magit://v1/Status` URI, unchanged server text and a visible editor. `r-vs-2-reopen.png`. |
./clients/vscode/NOTES.md:181:| Refresh | `g` sent `view/invoke common.refresh`; the server returned version 2 edits with `baseVersion: 1`; the document text exactly matched the applied render chain, `+after-return` was in a visible range, and the URI/tab stayed `magit://v1/Status` / `Status`. `r-vs-2-refresh.png`. |
./clients/vscode/NOTES.md:236:| Actual client case | Physical input/result | Magit requests | Inspected screenshot |
./clients/vscode/NOTES.md:238:| Foreign QuickPick | mirrored Ctrl+Alt+], `beta g`, Enter accepts `beta g` | 0 | `01-foreign-quickpick.png` |
./clients/vscode/NOTES.md:239:| Foreign InputBox | mirrored Ctrl+Alt+], `hello foreign`, Enter accepts text | 0 | `02-foreign-inputbox.png` |
./clients/vscode/NOTES.md:240:| Find | mirrored Ctrl+Alt+], `g`, Ctrl+A, `tracked`, Enter selects match; Ctrl+A/C copies `tracked`; Escape returns | 0 | `03-find-widget.png` |
./clients/vscode/NOTES.md:241:| Restored status | Enter reaches `common.visit` (planned-action message, not file visiting) | 1 | `04-status-enter.png` |
./clients/vscode/NOTES.md:242:| Restored status | Tab twice folds/unfolds the server section | 2 toggles | `05-status-fold.png` |
./clients/vscode/NOTES.md:243:| Restored status | mirrored Ctrl+Alt+] twice folds/unfolds the server section | 2 toggles | `05b-mirrored-fold.png` |
./clients/vscode/NOTES.md:244:| Restored status | `g` rereads Git and renders `+after-return` | 1 refresh | `06-status-refresh.png` |
./clients/vscode/NOTES.md:245:| Magit commit QuickPick | `c` opens; `-`, `a` turn `--all` on; `q` pops to status | `open.commit` plus 3 `view/key` | `07-magit-quickpick.png` |
./clients/vscode/NOTES.md:246:| F5 diagnostic | no refresh request, no `+f5-probe` render | 0 | `08-f5-before-dismissal.png`, `08-f5-probe.png` |
./clients/vscode/NOTES.md:248:All listed screenshots except the redundant post-dismissal F5 image were opened
./clients/vscode/NOTES.md:249:and visually inspected, along with `00-status-expanded.png` and `screen.png`:
./clients/vscode/NOTES.md:263:in the pre-dismissal screenshot. The interception/resolver cause is unproven;
./clients/vscode/NOTES.md:500:  from the pass/fail refresh path. It also captures per-step screenshots and
./clients/vscode/NOTES.md:503:  DISPLAY/binary precondition checks, screenshot listing, and targeted cleanup
./clients/vscode/NOTES.md:526:so status-text screenshots from `import` are not valid evidence. `F5` was not
./clients/vscode/NOTES.md:530:client scenario, verify the find-widget focus path, capture the screenshots and
./clients/vscode/test/e2e/run.mjs:35:  if (/image|screenshot|snapshot/i.test(method)) {
./clients/vscode/test/no-vision.js:21:    assert(!/\b(?:Page\.)?captureScreenshot\b|\b(?:writeFileSync|writeFile|readFileSync|readFile)\s*\([^\n]*\.(?:png|jpe?g|webp)\b/i.test(source), `${file} contains image capture/artifact I/O`);
./ui-test-plan.md:7:**No-vision policy:** Never use vision, inspect/render images, invoke image or screenshot tools, or capture screenshots. This applies to every agent and delegated worker. Native UI interaction remains required where specified, but checkpoints and acceptance use text/structured evidence only: tmux `.screen`, accessibility/DOM text, JSON, wire/API and Git state. Text snapshots remain allowed. Do not add image-snapshot tests or screenshot capture paths, and do not require future image inspection. Historical evidence descriptions remain historical; they are not claims that evidence was text-based.
./ui-test-plan.md:25:| L2 | mini.test child Neovim | Window counts, floats, keymaps applied, buffer options, `:messages`. Its screenshot cannot capture the command-line area reliably (neovim#21886), so read `:messages` instead |
./ui-test-plan.md:96:  history, version-error, dirty-close, abort and disconnect screenshots plus
./docs/evidence/commit-head-operations.md:193:`head-operations-evidence.json` (pass, eight isolated cases, 26 named screenshots)
./docs/evidence/commit-head-operations.md:229:Inspected current screenshots `head-amend-clean-saved.png`,
./docs/evidence/commit-head-operations.md:230:`head-amend-staged-status.png`, `head-reword-{saved,status}.png`,
./docs/evidence/commit-head-operations.md:231:`head-extend-tracked-all-status.png`, `head-extend-clean-guard-status.png`,
./docs/evidence/commit-head-operations.md:232:`head-extend-clean-allowed-menu.png`, and `head-amend-abort-edited.png`.
./docs/evidence/commit-head-operations.md:238:status screenshots.
./docs/evidence/commit-head-operations.md:245:screenshots exposed immediate subprocess exit truncating CDP JSON; awaiting
./docs/evidence/commit-head-operations.md:322:three `resetAuthorCases` and 39 named screenshots; the trace retains native
./docs/evidence/commit-head-operations.md:324:Inspected current `head-reset-author-amend-saved.png`,
./docs/evidence/commit-head-operations.md:325:`head-reset-author-extend-menu.png`, and
./docs/evidence/commit-head-operations.md:326:`head-reset-author-reword-{saved,status}.png`: the real saved subject/body,
./docs/evidence/real-file-client-spike.md:46:Inspected all three `pre-action.png` images: native Explorer, permanent draft
./docs/evidence/real-file-client-spike.md:130:Inspected both verified hosts' API/wire traces and the normal host screenshots:
./docs/evidence/real-file-client-spike.md:131:`03c-dirty-save-dialog.png` (Save focused; two dirty native text lines),
./docs/evidence/real-file-client-spike.md:132:`03e-dirty-discard-dialog.png` (actual Save/Don't Save/Cancel prompt over
./docs/evidence/real-file-client-spike.md:133:`discarded text`), `03b-dirty-cancel.png` (dirty tab retained),
./docs/evidence/real-file-client-spike.md:134:`03d-dirty-save-closed.png` and `03f-dirty-discard-closed.png` (no draft tab),
./docs/evidence/real-file-client-spike.md:135:and `04-refresh-language.png` (reopened, refreshed dirty plaintext editor).
./docs/evidence/real-file-client-spike.md:215:Inspected the normal host's `01-open.png`, `02-native-close.png`,
./docs/evidence/real-file-client-spike.md:216:`03-dirty-dialog.png` and `04-refresh-language.png`: real file/text visible;
./docs/RELEASING.md:10:screenshot fallback are in [SCREENSHOTS](SCREENSHOTS.md). External adapters use
./docs/RELEASING.md:15:client evidence. Native screenshots are captured on demand, not by a new CI job.
./docs/RELEASING.md:42:  `a06-evidence.json`. Opened `r-vs-2-first-open.png`,
./docs/RELEASING.md:43:  `a06-menu-before-quit.png` and `a06-reopened.png` (1192x756, dark theme).
./docs/RELEASING.md:46:  not complete menu presentation or release-ready product screenshots.
./docs/CLIENT_INSTALL.md:132:published-VSIX evidence; those release gates remain open. Native screenshots
./docs/SCREENSHOTS.md:1:# Native screenshot capture and manual fallback
./docs/SCREENSHOTS.md:9:or a mock editor into a purported native screenshot. Keep original artifacts
./docs/SCREENSHOTS.md:15:is headless evidence, not native screenshot evidence or a published install.
./docs/SCREENSHOTS.md:22:desktop screenshot tool if a PNG is needed. The harness's `.screen` and `.ansi`
./docs/SCREENSHOTS.md:63:not publish a synthesized image of `.screen` as a native terminal screenshot.
./docs/SCREENSHOTS.md:91:`r-vs-2-first-open.png`, `a06-menu-before-quit.png` and `a06-reopened.png` there;
./docs/SCREENSHOTS.md:93:focus, readable layout and absence of errors. `failure.txt`/`failure-screen.png`
./docs/SCREENSHOTS.md:94:are diagnostic evidence, not successful product screenshots.
./docs/SCREENSHOTS.md:105:window with a desktop screenshot tool. Retain commands, settings, versions and
./docs/SCREENSHOTS.md:106:screenshots; report manual evidence separately from automated e2e success.
./docs/SCREENSHOTS.md:107:Do not attempt commit editing merely to capture status/menu screenshots.
./AGENTS.md:65:- **No-vision policy (mandatory for every agent, including delegated workers and subagents):** never use vision, inspect or render images, or invoke image/screenshot tools, under any circumstances. Do not capture screenshots or add image-snapshot tests/capture paths. Native UI verification remains real and interactive, but evidence and acceptance must be text/structured state only (for example `.screen`, accessibility/DOM text, JSON, wire/API and Git state). Text snapshots are allowed. Do not impose image-based acceptance or future screenshot inspection requirements.
./AGENTS.md:116:  scenario's `failure.screen` only if its inline text output is insufficient. Never use screenshot/image output.
```
(exit 0)



Color-spike path/output classification, checked against source: its header says
`tohtml` skips the semantic-token namespace and HTML is “only a reference”; the
token table, not HTML, is the result (`scripts/color_spike.lua:9-11`). The six
configured HTML cases are `default/dark`, `default/light`, `vim/dark`,
`habamax/dark`, `delek/dark`, and `lunaperche/dark` (`:256-262`); each is written
as `build/test-output/status-<scheme>-<background>.html` (`:267-288`). The
seventh intended output is `build/test-output/semantic-tokens.md` (`:388-400`),
a Markdown legend. Because setup failed at line 193 before the case loop, no
HTML or Markdown output was emitted in this attempt. The setup failure is the
only observed failure; no fallback output is claimed.

Full `scripts/color_spike.lua` function/export index from
`grep -nE '^(local function|function |local [[:alnum:]_]+ = function)'`: `sys`
24, `make_fixture` 32, `hex` 72, `resolved_name` 79, `probe_tohtml` 91,
`collect_tokens` 164, local `pad` 309. Loops/report blocks: tokens 228,
token types 237, modifiers 244, cases 267, columns/table report 305-413;
no exported module functions. Full callable index for `scripts/ui_lib/` is
reproducible with `grep -nE '^    def |^def |^class ' scripts/ui_lib/*.py`;
`cli.py` 26-199, `decoder.py` 36-212, `driver.py` 29-895, `fixtures.py` 68-262,
`paths.py` 13, `scenario.py` 35-1095, `startup.py` 26-201. Existing detailed
indexes above are excerpts; these ranges denote all indexed declarations, not
full implementation transcriptions of the long Python modules.

### Reproducible full declaration indexes

Complete callable declaration index, requested long Python helpers:

```text
$ grep -nE '^    def |^def |^class ' scripts/ui_lib/*.py
scripts/ui_lib/cli.py:26:def _in_ci() -> bool:
scripts/ui_lib/cli.py:31:def _take_run(args: list[str]) -> tuple[str | None, list[str]]:
scripts/ui_lib/cli.py:53:def _take_value(args: list[str], flag: str, default: str | None = None) -> tuple[str | None, list[str]]:
scripts/ui_lib/cli.py:75:def _start(args: list[str]) -> int:
scripts/ui_lib/cli.py:96:def _keys(args: list[str]) -> int:
scripts/ui_lib/cli.py:105:def _wait(args: list[str]) -> int:
scripts/ui_lib/cli.py:125:def _screen(args: list[str]) -> int:
scripts/ui_lib/cli.py:138:def _state(args: list[str]) -> int:
scripts/ui_lib/cli.py:147:def _trace(args: list[str]) -> int:
scripts/ui_lib/cli.py:165:def _stop(args: list[str]) -> int:
scripts/ui_lib/cli.py:173:def _run(args: list[str]) -> int:
scripts/ui_lib/cli.py:199:def main(argv: list[str] | None = None) -> int:
scripts/ui_lib/decoder.py:36:def _style(fg, bg, bold, dim, reverse) -> tuple:
scripts/ui_lib/decoder.py:40:def _apply_sgr(
scripts/ui_lib/decoder.py:97:def _extended_colour(values: list[int], index: int) -> tuple[str | None, int]:
scripts/ui_lib/decoder.py:112:def decode(text: str) -> list[dict]:
scripts/ui_lib/decoder.py:121:    def flush() -> None:
scripts/ui_lib/decoder.py:127:    def emit(chunk: str) -> None:
scripts/ui_lib/decoder.py:212:def runs_containing(runs: Iterable[dict], needle: str) -> list[dict]:
scripts/ui_lib/driver.py:29:class DriverError(RuntimeError):
scripts/ui_lib/driver.py:33:class StartupError(DriverError):
scripts/ui_lib/driver.py:41:class SkipError(DriverError):
scripts/ui_lib/driver.py:46:class Run:
scripts/ui_lib/driver.py:63:    def dir(self) -> Path:
scripts/ui_lib/driver.py:68:    def repo_path(self) -> Path:
scripts/ui_lib/driver.py:73:    def home_path(self) -> Path:
scripts/ui_lib/driver.py:78:    def socket_path(self) -> Path:
scripts/ui_lib/driver.py:83:    def trace_path(self) -> Path:
scripts/ui_lib/driver.py:87:    def save(self) -> None:
scripts/ui_lib/driver.py:93:    def load(cls, run_id: str) -> "Run":
scripts/ui_lib/driver.py:105:def check_tools() -> None:
scripts/ui_lib/driver.py:112:def parse_size(value: str) -> tuple[int, int]:
scripts/ui_lib/driver.py:123:def new_run_id() -> str:
scripts/ui_lib/driver.py:129:def _update_last_run(run_id: str) -> None:
scripts/ui_lib/driver.py:134:def latest_run_id() -> str:
scripts/ui_lib/driver.py:147:def load_run(run_id: str | None) -> Run:
scripts/ui_lib/driver.py:152:def _forget_last_run(run_id: str) -> None:
scripts/ui_lib/driver.py:166:def _tmux(run: Run, *args: str, check: bool = True) -> subprocess.CompletedProcess:
scripts/ui_lib/driver.py:179:def _nvim(run: Run, *args: str, timeout: float = 5.0) -> subprocess.CompletedProcess:
scripts/ui_lib/driver.py:189:def _expr(run: Run, expression: str, timeout: float = 5.0) -> str | None:
scripts/ui_lib/driver.py:237:def snapshot(run: Run) -> dict:
scripts/ui_lib/driver.py:248:def state_json(run: Run, indent: int | None = 2) -> str:
scripts/ui_lib/driver.py:256:def idle(run: Run) -> bool:
scripts/ui_lib/driver.py:261:def _idle_probe(run: Run) -> startup.IdleProbe:
scripts/ui_lib/driver.py:285:def _decode_output(output: str | bytes | None) -> str:
scripts/ui_lib/driver.py:292:def _client_startup_state(run: Run) -> list[str]:
scripts/ui_lib/driver.py:344:def _safe_text(value: object, limit: int = 400) -> str:
scripts/ui_lib/driver.py:351:def document(run: Run) -> str:
scripts/ui_lib/driver.py:369:def goto_text(run: Run, text: str) -> None:
scripts/ui_lib/driver.py:384:def clear_message_line(run: Run) -> None:
scripts/ui_lib/driver.py:393:def prompt_pending(run: Run) -> bool:
scripts/ui_lib/driver.py:398:def wait_prompt(run: Run, timeout_ms: int = 2000, poll_ms: int = 20) -> bool:
scripts/ui_lib/driver.py:412:def wait_idle(run: Run, timeout_ms: int = 2000, settle_ms: int = 50) -> bool:
scripts/ui_lib/driver.py:434:def capture(run: Run, ansi: bool = False) -> str:
scripts/ui_lib/driver.py:445:def wait_screen(run: Run, text: str, timeout_ms: int = SCREEN_TIMEOUT_MS) -> bool:
scripts/ui_lib/driver.py:470:def is_key_name(token: str) -> bool:
scripts/ui_lib/driver.py:514:def _csi_u_bytes(token: str) -> list[str] | None:
scripts/ui_lib/driver.py:541:def send_keys(run: Run, tokens: list[str]) -> None:
scripts/ui_lib/driver.py:557:def _pane_dead(run: Run) -> bool:
scripts/ui_lib/driver.py:564:def _wait_for_socket(run: Run, timeout_ms: int = SOCKET_TIMEOUT_MS) -> None:
scripts/ui_lib/driver.py:591:def _wait_for_view(run: Run, timeout_ms: int = START_TIMEOUT_MS) -> None:
scripts/ui_lib/driver.py:614:def _cleanup(run: Run) -> None:
scripts/ui_lib/driver.py:624:def _launcher_path(run: Run) -> Path:
scripts/ui_lib/driver.py:629:def _stderr_path(run: Run) -> Path:
scripts/ui_lib/driver.py:634:def _exit_path(run: Run) -> Path:
scripts/ui_lib/driver.py:639:def _shell() -> str:
scripts/ui_lib/driver.py:644:def _nvim_argv(run: Run) -> list[str]:
scripts/ui_lib/driver.py:661:def _launcher_script(run: Run, env: dict[str, str]) -> Path:
scripts/ui_lib/driver.py:692:def _launcher_command(run: Run) -> str:
scripts/ui_lib/driver.py:697:def startup_report(
scripts/ui_lib/driver.py:732:def start(
scripts/ui_lib/driver.py:844:def stop(run: Run) -> None:
scripts/ui_lib/driver.py:867:def _read_events(path: Path) -> list[dict]:
scripts/ui_lib/driver.py:880:def read_trace(run: Run) -> list[dict]:
scripts/ui_lib/driver.py:895:def format_trace(events: list[dict], count: int | None = None) -> str:
scripts/ui_lib/fixtures.py:68:def determinism_env(home: Path) -> dict[str, str]:
scripts/ui_lib/fixtures.py:106:def git(
scripts/ui_lib/fixtures.py:127:def write(path: Path, content: str) -> None:
scripts/ui_lib/fixtures.py:133:def commit(repo: Path, env: dict[str, str], message: str, files: dict[str, str]) -> str:
scripts/ui_lib/fixtures.py:142:def _init(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:146:def _clean(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:150:def _modified_two_files(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:156:def _staged_one_file(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:162:def _staged_and_unstaged(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:169:def _staged_failing_hook(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:183:def _staged_slow_hook(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:201:def _untracked(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:206:def _unborn(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:211:def _in_progress_rebase(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:225:def _in_progress_cherry_pick(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:238:def _log_history(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:262:def build(name: str, dest: Path, home: Path | None = None) -> Path:
scripts/ui_lib/paths.py:13:def run_dir(run_id: str) -> Path:
scripts/ui_lib/scenario.py:35:class ScenarioError(RuntimeError):
scripts/ui_lib/scenario.py:39:class ShellStepError(ScenarioError):
scripts/ui_lib/scenario.py:44:class Expectation:
scripts/ui_lib/scenario.py:60:class Statement:
scripts/ui_lib/scenario.py:74:class Scenario:
scripts/ui_lib/scenario.py:91:class ComboResult:
scripts/ui_lib/scenario.py:103:class Row:
scripts/ui_lib/scenario.py:117:def strip_comment(line: str) -> str:
scripts/ui_lib/scenario.py:146:def parse_bool(value: str) -> bool:
scripts/ui_lib/scenario.py:156:def unquote(text: str) -> str:
scripts/ui_lib/scenario.py:164:def parse_value(text: str) -> Any:
scripts/ui_lib/scenario.py:184:def parse_keys(text: str) -> list[str]:
scripts/ui_lib/scenario.py:195:def parse_env(text: str) -> dict[str, str]:
scripts/ui_lib/scenario.py:212:def parse_expectation(line: str) -> Expectation:
scripts/ui_lib/scenario.py:276:def parse_statement_name(line: str, keyword: str, where: str) -> str:
scripts/ui_lib/scenario.py:286:def parse_statement(line: str, where: str) -> Statement:
scripts/ui_lib/scenario.py:337:def parse_scenario(path: Path) -> Scenario:
scripts/ui_lib/scenario.py:416:def _format_value(value: Any) -> str:
scripts/ui_lib/scenario.py:420:def resolve_path(path: str, snapshot: dict) -> Any:
scripts/ui_lib/scenario.py:454:def check_screen_contains(screen: str, text: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:461:def check_screen_not_contains(screen: str, text: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:469:def check_screen_matches(screen: str, pattern: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:480:def _row_view(runs: Iterable[dict], row: int) -> tuple[str, list[dict]]:
scripts/ui_lib/scenario.py:494:def _overlapping_runs(
scripts/ui_lib/scenario.py:519:def check_style(ansi: str, text: str, mode: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:545:def check_no_message(snapshot: dict, pattern: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:560:def check_cursor(snapshot: dict, screen: str, text: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:576:def measure_latency(key: str, events: list[dict]) -> float | None:
scripts/ui_lib/scenario.py:604:def budget_scale() -> float:
scripts/ui_lib/scenario.py:615:def strict_latency() -> bool:
scripts/ui_lib/scenario.py:620:def check_latency(
scripts/ui_lib/scenario.py:644:def format_expectation(expectation: Expectation) -> str:
scripts/ui_lib/scenario.py:668:def check_document_unchanged(
scripts/ui_lib/scenario.py:682:def evaluate(
scripts/ui_lib/scenario.py:732:def combinations(scenario: Scenario) -> list[tuple[str, str]]:
scripts/ui_lib/scenario.py:756:def classify(xfail: str | None, results: list[ComboResult]) -> str:
scripts/ui_lib/scenario.py:767:def _snapshot_paths(scenario: Scenario, profile: str, size: str, name: str) -> tuple[Path, Path]:
scripts/ui_lib/scenario.py:775:def _write_snapshot(
scripts/ui_lib/scenario.py:792:def _clear_command_line(run: driver.Run) -> None:
scripts/ui_lib/scenario.py:804:def _run_shell(run: driver.Run, command: str, where: int) -> None:
scripts/ui_lib/scenario.py:843:def _shell_output(output: str | bytes | None) -> str:
scripts/ui_lib/scenario.py:851:def _run_combo(
scripts/ui_lib/scenario.py:996:def _trim_screen(screen: str) -> str:
scripts/ui_lib/scenario.py:1004:def _save_failure_screen(scenario: Scenario, result: ComboResult) -> Path:
scripts/ui_lib/scenario.py:1012:def _report_failure(
scripts/ui_lib/scenario.py:1037:def _print_table(rows: list[Row], verbose: bool = False) -> None:
scripts/ui_lib/scenario.py:1065:def _write_markdown(path: Path, snapshots: list[tuple[str, str, str, str, str]]) -> None:
scripts/ui_lib/scenario.py:1083:def expand_paths(paths: Iterable[str]) -> list[Path]:
scripts/ui_lib/scenario.py:1095:def run(paths: Iterable[str], markdown: str | None = None, verbose: bool = False) -> int:
scripts/ui_lib/startup.py:26:class SocketState:
scripts/ui_lib/startup.py:37:    def too_long(self) -> bool:
scripts/ui_lib/startup.py:41:    def summary(self) -> str:
scripts/ui_lib/startup.py:61:class ConnectState:
scripts/ui_lib/startup.py:68:    def summary(self) -> str:
scripts/ui_lib/startup.py:76:class IdleProbe:
scripts/ui_lib/startup.py:85:    def summary(self) -> str:
scripts/ui_lib/startup.py:109:def _bounded(text: str) -> str:
scripts/ui_lib/startup.py:117:def inspect_socket(path: Path) -> SocketState:
scripts/ui_lib/startup.py:140:def read_tail(path: Path, lines: int = STDERR_TAIL_LINES) -> list[str]:
scripts/ui_lib/startup.py:150:def read_exit_status(path: Path) -> int | None:
scripts/ui_lib/startup.py:162:def connect_probe(
scripts/ui_lib/startup.py:201:def format_report(
```
(exit 0)

Complete `debug.lua` exported/local function line index:

```text
$ grep -nE '^(local function|function M\.|M\.[[:alnum:]_]+ =|local M)' lua/magit-lsp/debug.lua
9:local M = {}
14:M.state = {
25:M.views = {}
28:M.order = {}
31:M.notifications = {}
34:M.progress_events = {}
60:function M.trace(event, detail)
81:function M.record_notification(msg)
90:function M.configure(opts)
125:function M.view_open(id, kind, buf)
142:function M.view_close(id)
158:function M.render_received(id, version)
165:function M.render_applied(id, version)
176:function M.render_acked(id, version)
186:function M.keymap_received(id, notice)
220:function M.set_pending(id, pending)
229:function M.key_pressed(steps)
236:function M.trace_id()
243:function M.progress_started(token, value)
251:function M.record_progress(token, value)
263:function M.progress_reported(token, value)
273:function M.progress_finished(token)
281:function M.key_sent(step, id)
286:function M.request_started()
291:function M.request_finished()
296:function M.queued_started()
301:function M.queued_finished()
306:function M.prompt_started()
311:function M.prompt_finished()
317:function M.schedule(fn)
330:function M.idle()
358:local function active_view()
371:local function window_state(win)
400:local function recent_messages()
418:function M.snapshot()
```
(exit 0)

`TestSession` relevant public signatures and internal dispatch/result functions:

```text
$ grep -nE '^    pub fn (press|invoke|ack|open_view|open_status|set_mode)|^    fn (press|invoke|ack|open_view|render)' tests/common/session.rs
110:    pub fn open_status(fixture: &RepoFixture, profile: Profile) -> (Self, ViewRender) {
115:    pub fn open_status_at(
126:    pub fn open_status_view(&mut self, viewport: Viewport) -> ViewRender {
132:    pub fn open_view(
150:    pub fn open_view_with_args(
216:    pub fn acked_version(&self) -> Option<u64> {
221:    pub fn acked_version_of(&self, view_id: &str) -> Option<u64> {
229:    pub fn ack(&mut self) {
234:    pub fn ack_view(&mut self, view_id: &str) {
369:    pub fn acked_tree(&self) -> &Tree {
448:    pub fn press(&mut self, keys: &str) -> InvokeResult {
459:    pub fn press_full(
485:    pub fn press_at_version(&mut self, key: &str, version: u64) -> InvokeResult {
504:    pub fn press_visual(&mut self, start: usize, end: usize, key: &str) -> InvokeResult {
520:    pub fn press_ui(&mut self, key: &str) -> InvokeResult {
543:    pub fn invoke(&mut self, command: &str) -> InvokeResult {
548:    pub fn invoke_args(&mut self, command: &str, args: Value) -> InvokeResult {
553:    pub fn invoke_at(&mut self, command: &str, cursor: Cursor) -> InvokeResult {
558:    pub fn invoke_at_args(
568:    pub fn invoke_here(&mut self, command: &str) -> InvokeResult {
573:    pub fn invoke_selection(
583:    pub fn invoke_ui(&mut self, command: &str) -> InvokeResult {
619:    pub fn invoke_full(
631:    pub fn invoke_at_version(
656:    pub fn set_mode(&mut self, mode: ViewMode) -> InvokeResult {
```
(exit 0)

Complete `color_spike.lua` function index:

```text
$ grep -nE '^(local function|function |local [[:alnum:]_]+ = function)' scripts/color_spike.lua
24:local function sys(cmd)
32:local function make_fixture()
72:local function hex(n)
79:local function resolved_name(group)
91:local function probe_tohtml()
164:local function collect_tokens(buf, lines)
309:local function pad(s, n)
```
(exit 0)

## B. VSCode — source evidence (checkout-relative; home/user values redacted only)

Commands used: `find clients/vscode/{lib,test,scripts} -type f | sort`; `wc -l clients/vscode/lib/*.js clients/vscode/test/*.js clients/vscode/test/fake/*.js clients/vscode/test/e2e/* clients/vscode/scripts/*`; `rg -n "test\(|function |exports\.|process\.env|window\.dialogStyle|view/open|wire\.jsonl" clients/vscode build`; `nl -ba` on the sources excerpted below. Line-numbered text is literal source (prefix is index, not source).

### Manifest excerpts

### `clients/vscode/package.json` lines 1-27 of 1838
```text
   1 | {
   2 |   "name": "magit-lsp",
   3 |   "displayName": "magit-lsp",
   4 |   "description": "Magit-style Git views rendered by the magit-lsp language server",
   5 |   "version": "0.1.0",
   6 |   "publisher": "magit-lsp",
   7 |   "private": true,
   8 |   "license": "Apache-2.0",
   9 |   "engines": {
  10 |     "vscode": "^1.85.0"
  11 |   },
  12 |   "categories": [
  13 |     "Other"
  14 |   ],
  15 |   "activationEvents": [
  16 |     "onCommand:magit.open",
  17 |     "onCommand:magit.key",
  18 |     "onCommand:magit.invoke",
  19 |     "onCommand:magit.mirrorFoldShortcuts"
  20 |   ],
  21 |   "main": "./extension.js",
  22 |   "scripts": {
  23 |     "gen:keybindings": "node scripts/gen-keybindings.mjs",
  24 |     "test": "node test/test.js && node test/integration.js",
  25 |     "test:unit": "node test/test.js",
  26 |     "test:integration": "node test/integration.js",
  27 |     "test:e2e": "node test/e2e/run.mjs"
```
`activationEvents` and scripts are verbatim above. Contribution manifests occupy additional package ranges and include keybindings/configuration; source line index from `rg -n '"contributes"|"commands"|"keybindings"|"configuration"' clients/vscode/package.json`: 36:  "contributes": {
120:    "commands": [
179:    "keybindings": [
1813:    "configuration": {

### Full package manifest (contributes, activationEvents and scripts verbatim)
```text
   1 | {
   2 |   "name": "magit-lsp",
   3 |   "displayName": "magit-lsp",
   4 |   "description": "Magit-style Git views rendered by the magit-lsp language server",
   5 |   "version": "0.1.0",
   6 |   "publisher": "magit-lsp",
   7 |   "private": true,
   8 |   "license": "Apache-2.0",
   9 |   "engines": {
  10 |     "vscode": "^1.85.0"
  11 |   },
  12 |   "categories": [
  13 |     "Other"
  14 |   ],
  15 |   "activationEvents": [
  16 |     "onCommand:magit.open",
  17 |     "onCommand:magit.key",
  18 |     "onCommand:magit.invoke",
  19 |     "onCommand:magit.mirrorFoldShortcuts"
  20 |   ],
  21 |   "main": "./extension.js",
  22 |   "scripts": {
  23 |     "gen:keybindings": "node scripts/gen-keybindings.mjs",
  24 |     "test": "node test/test.js && node test/integration.js",
  25 |     "test:unit": "node test/test.js",
  26 |     "test:integration": "node test/integration.js",
  27 |     "test:e2e": "node test/e2e/run.mjs"
  28 |   },
  29 |   "dependencies": {
  30 |     "jsonc-parser": "^3.3.1",
  31 |     "vscode-languageclient": "^9.0.1"
  32 |   },
  33 |   "devDependencies": {
  34 |     "typescript": "^6.0.3"
  35 |   },
  36 |   "contributes": {
  37 |     "languages": [
  38 |       {
  39 |         "id": "magit",
  40 |         "aliases": [
  41 |           "Magit",
  42 |           "magit"
  43 |         ]
  44 |       }
  45 |     ],
  46 |     "configurationDefaults": {
  47 |       "[magit]": {
  48 |         "editor.semanticHighlighting.enabled": true
  49 |       }
  50 |     },
  51 |     "semanticTokenTypes": [
  52 |       {
  53 |         "id": "magit-section",
  54 |         "superType": "namespace",
  55 |         "description": "Status section heading"
  56 |       },
  57 |       {
  58 |         "id": "magit-diff-added",
  59 |         "description": "Added diff line"
  60 |       },
  61 |       {
  62 |         "id": "magit-diff-removed",
  63 |         "description": "Removed diff line"
  64 |       },
  65 |       {
  66 |         "id": "magit-message-error",
  67 |         "description": "Error output from a failed command"
  68 |       },
  69 |       {
  70 |         "id": "magit-message-warn",
  71 |         "description": "Warning output from a successful command"
  72 |       }
  73 |     ],
  74 |     "semanticTokenScopes": [
  75 |       {
  76 |         "language": "magit",
  77 |         "scopes": {
  78 |           "magit-section": [
  79 |             "entity.name.section"
  80 |           ],
  81 |           "magit-count": [
  82 |             "constant.numeric"
  83 |           ],
  84 |           "magit-file": [
  85 |             "string"
  86 |           ],
  87 |           "magit-file-modified": [
  88 |             "markup.changed"
  89 |           ],
  90 |           "magit-file-added": [
  91 |             "markup.inserted"
  92 |           ],
  93 |           "magit-file-deleted": [
  94 |             "markup.deleted"
  95 |           ],
  96 |           "magit-file-renamed": [
  97 |             "markup.changed"
  98 |           ],
  99 |           "magit-file-untracked": [
 100 |             "comment"
 101 |           ],
 102 |           "magit-hunk-header": [
 103 |             "meta.diff.range"
 104 |           ],
 105 |           "magit-diff-added": [
 106 |             "markup.inserted"
 107 |           ],
 108 |           "magit-diff-removed": [
 109 |             "markup.deleted"
 110 |           ],
 111 |           "magit-message-error": [
 112 |             "invalid"
 113 |           ],
 114 |           "magit-message-warn": [
 115 |             "markup.warning"
 116 |           ]
 117 |         }
 118 |       }
 119 |     ],
 120 |     "commands": [
 121 |       {
 122 |         "command": "magit.confirmEdit",
 123 |         "title": "Magit: Confirm Commit Draft",
 124 |         "enablement": "resourceScheme == magit-commit",
 125 |         "icon": "$(check)"
 126 |       },
 127 |       {
 128 |         "command": "magit.cancelEdit",
 129 |         "title": "Magit: Abort Commit Draft",
 130 |         "enablement": "resourceScheme == magit-commit"
 131 |       },
 132 |       {
 133 |         "command": "magit.closeEdit",
 134 |         "title": "Magit: Close Commit Draft (Saved Checkpoint)",
 135 |         "enablement": "resourceScheme == magit-commit",
 136 |         "icon": "$(close)"
 137 |       },
 138 |       {
 139 |         "command": "magit.open",
 140 |         "title": "Open Status",
 141 |         "category": "Magit"
 142 |       },
 143 |       {
 144 |         "command": "magit.key",
 145 |         "title": "Press Key",
 146 |         "category": "Magit",
 147 |         "enablement": "magit.active"
 148 |       },
 149 |       {
 150 |         "command": "magit.invoke",
 151 |         "title": "Invoke Command",
 152 |         "category": "Magit",
 153 |         "enablement": "magit.active"
 154 |       },
 155 |       {
 156 |         "command": "magit.mirrorFoldShortcuts",
 157 |         "title": "Mirror my fold shortcuts",
 158 |         "category": "Magit"
 159 |       }
 160 |     ],
 161 |     "menus": {
 162 |       "editor/title": [
 163 |         {
 164 |           "command": "magit.confirmEdit",
 165 |           "when": "resourceScheme == magit-commit",
 166 |           "group": "navigation@1"
 167 |         },
 168 |         {
 169 |           "command": "magit.closeEdit",
 170 |           "when": "resourceScheme == magit-commit",
 171 |           "group": "navigation@2"
 172 |         },
 173 |         {
 174 |           "command": "magit.cancelEdit",
 175 |           "when": "resourceScheme == magit-commit"
 176 |         }
 177 |       ]
 178 |     },
 179 |     "keybindings": [
 180 |       {
 181 |         "key": "f5",
 182 |         "command": "magit.invoke",
 183 |         "args": {
 184 |           "command": "common.refresh"
 185 |         },
 186 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 187 |       },
 188 |       {
 189 |         "key": "g",
 190 |         "command": "magit.invoke",
 191 |         "args": {
 192 |           "command": "common.refresh"
 193 |         },
 194 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 195 |       },
 196 |       {
 197 |         "key": "tab",
 198 |         "command": "magit.invoke",
 199 |         "args": {
 200 |           "command": "common.toggle-section"
 201 |         },
 202 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 203 |       },
 204 |       {
 205 |         "key": "alt+1",
 206 |         "command": "magit.invoke",
 207 |         "args": {
 208 |           "command": "common.detail-level-1"
 209 |         },
 210 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 211 |       },
 212 |       {
 213 |         "key": "alt+2",
 214 |         "command": "magit.invoke",
 215 |         "args": {
 216 |           "command": "common.detail-level-2"
 217 |         },
 218 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 219 |       },
 220 |       {
 221 |         "key": "alt+3",
 222 |         "command": "magit.invoke",
 223 |         "args": {
 224 |           "command": "common.detail-level-3"
 225 |         },
 226 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 227 |       },
 228 |       {
 229 |         "key": "alt+4",
 230 |         "command": "magit.invoke",
 231 |         "args": {
 232 |           "command": "common.detail-level-4"
 233 |         },
 234 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 235 |       },
 236 |       {
 237 |         "key": "n",
 238 |         "command": "magit.invoke",
 239 |         "args": {
 240 |           "command": "common.next-section"
 241 |         },
 242 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 243 |       },
 244 |       {
 245 |         "key": "p",
 246 |         "command": "magit.invoke",
 247 |         "args": {
 248 |           "command": "common.prev-section"
 249 |         },
 250 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 251 |       },
 252 |       {
 253 |         "key": "escape",
 254 |         "command": "magit.invoke",
 255 |         "args": {
 256 |           "command": "common.cancel"
 257 |         },
 258 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 259 |       },
 260 |       {
 261 |         "key": "ctrl+c",
 262 |         "command": "magit.invoke",
 263 |         "args": {
 264 |           "command": "common.cancel"
 265 |         },
 266 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 267 |       },
 268 |       {
 269 |         "key": "enter",
 270 |         "command": "magit.invoke",
 271 |         "args": {
 272 |           "command": "common.confirm"
 273 |         },
 274 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 275 |       },
 276 |       {
 277 |         "key": "q",
 278 |         "command": "magit.invoke",
 279 |         "args": {
 280 |           "command": "common.quit"
 281 |         },
 282 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 283 |       },
 284 |       {
 285 |         "key": "shift+tab",
 286 |         "command": "magit.invoke",
 287 |         "args": {
 288 |           "command": "common.cycle-visibility"
 289 |         },
 290 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 291 |       },
 292 |       {
 293 |         "key": "alt+down",
 294 |         "command": "magit.invoke",
 295 |         "args": {
 296 |           "command": "common.next-sibling"
 297 |         },
 298 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 299 |       },
 300 |       {
 301 |         "key": "alt+up",
 302 |         "command": "magit.invoke",
 303 |         "args": {
 304 |           "command": "common.prev-sibling"
 305 |         },
 306 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 307 |       },
 308 |       {
 309 |         "key": "alt+left",
 310 |         "command": "magit.invoke",
 311 |         "args": {
 312 |           "command": "common.parent-section"
 313 |         },
 314 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 315 |       },
 316 |       {
 317 |         "key": "ctrl+t",
 318 |         "command": "magit.invoke",
 319 |         "args": {
 320 |           "command": "common.toggle-text-mode"
 321 |         },
 322 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 323 |       },
 324 |       {
 325 |         "key": "ctrl+t",
 326 |         "command": "magit.invoke",
 327 |         "args": {
 328 |           "command": "common.toggle-text-mode"
 329 |         },
 330 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && magit.textMode"
 331 |       },
 332 |       {
 333 |         "key": "ctrl+shift+c",
 334 |         "command": "magit.invoke",
 335 |         "args": {
 336 |           "command": "common.copy-value"
 337 |         },
 338 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 339 |       },
 340 |       {
 341 |         "key": "shift+/",
 342 |         "command": "magit.invoke",
 343 |         "args": {
 344 |           "command": "open.dispatch"
 345 |         },
 346 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'common' && !magit.textMode"
 347 |       },
 348 |       {
 349 |         "key": "f5",
 350 |         "command": "magit.invoke",
 351 |         "args": {
 352 |           "command": "common.refresh"
 353 |         },
 354 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 355 |       },
 356 |       {
 357 |         "key": "g",
 358 |         "command": "magit.invoke",
 359 |         "args": {
 360 |           "command": "common.refresh"
 361 |         },
 362 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 363 |       },
 364 |       {
 365 |         "key": "tab",
 366 |         "command": "magit.invoke",
 367 |         "args": {
 368 |           "command": "common.toggle-section"
 369 |         },
 370 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 371 |       },
 372 |       {
 373 |         "key": "alt+1",
 374 |         "command": "magit.invoke",
 375 |         "args": {
 376 |           "command": "common.detail-level-1"
 377 |         },
 378 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 379 |       },
 380 |       {
 381 |         "key": "alt+2",
 382 |         "command": "magit.invoke",
 383 |         "args": {
 384 |           "command": "common.detail-level-2"
 385 |         },
 386 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 387 |       },
 388 |       {
 389 |         "key": "alt+3",
 390 |         "command": "magit.invoke",
 391 |         "args": {
 392 |           "command": "common.detail-level-3"
 393 |         },
 394 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 395 |       },
 396 |       {
 397 |         "key": "alt+4",
 398 |         "command": "magit.invoke",
 399 |         "args": {
 400 |           "command": "common.detail-level-4"
 401 |         },
 402 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 403 |       },
 404 |       {
 405 |         "key": "n",
 406 |         "command": "magit.invoke",
 407 |         "args": {
 408 |           "command": "common.next-section"
 409 |         },
 410 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 411 |       },
 412 |       {
 413 |         "key": "p",
 414 |         "command": "magit.invoke",
 415 |         "args": {
 416 |           "command": "common.prev-section"
 417 |         },
 418 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 419 |       },
 420 |       {
 421 |         "key": "s",
 422 |         "command": "magit.invoke",
 423 |         "args": {
 424 |           "command": "status.stage"
 425 |         },
 426 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 427 |       },
 428 |       {
 429 |         "key": "u",
 430 |         "command": "magit.invoke",
 431 |         "args": {
 432 |           "command": "status.unstage"
 433 |         },
 434 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 435 |       },
 436 |       {
 437 |         "key": "shift+s",
 438 |         "command": "magit.invoke",
 439 |         "args": {
 440 |           "command": "status.stage-all"
 441 |         },
 442 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 443 |       },
 444 |       {
 445 |         "key": "shift+u",
 446 |         "command": "magit.invoke",
 447 |         "args": {
 448 |           "command": "status.unstage-all"
 449 |         },
 450 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 451 |       },
 452 |       {
 453 |         "key": "escape",
 454 |         "command": "magit.invoke",
 455 |         "args": {
 456 |           "command": "common.cancel"
 457 |         },
 458 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 459 |       },
 460 |       {
 461 |         "key": "ctrl+c",
 462 |         "command": "magit.invoke",
 463 |         "args": {
 464 |           "command": "common.cancel"
 465 |         },
 466 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 467 |       },
 468 |       {
 469 |         "key": "q",
 470 |         "command": "magit.invoke",
 471 |         "args": {
 472 |           "command": "common.quit"
 473 |         },
 474 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 475 |       },
 476 |       {
 477 |         "key": "shift+tab",
 478 |         "command": "magit.invoke",
 479 |         "args": {
 480 |           "command": "common.cycle-visibility"
 481 |         },
 482 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 483 |       },
 484 |       {
 485 |         "key": "alt+down",
 486 |         "command": "magit.invoke",
 487 |         "args": {
 488 |           "command": "common.next-sibling"
 489 |         },
 490 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 491 |       },
 492 |       {
 493 |         "key": "alt+up",
 494 |         "command": "magit.invoke",
 495 |         "args": {
 496 |           "command": "common.prev-sibling"
 497 |         },
 498 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 499 |       },
 500 |       {
 501 |         "key": "alt+left",
 502 |         "command": "magit.invoke",
 503 |         "args": {
 504 |           "command": "common.parent-section"
 505 |         },
 506 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 507 |       },
 508 |       {
 509 |         "key": "ctrl+t",
 510 |         "command": "magit.invoke",
 511 |         "args": {
 512 |           "command": "common.toggle-text-mode"
 513 |         },
 514 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 515 |       },
 516 |       {
 517 |         "key": "ctrl+t",
 518 |         "command": "magit.invoke",
 519 |         "args": {
 520 |           "command": "common.toggle-text-mode"
 521 |         },
 522 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && magit.textMode"
 523 |       },
 524 |       {
 525 |         "key": "ctrl+shift+c",
 526 |         "command": "magit.invoke",
 527 |         "args": {
 528 |           "command": "common.copy-value"
 529 |         },
 530 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 531 |       },
 532 |       {
 533 |         "key": "enter",
 534 |         "command": "magit.invoke",
 535 |         "args": {
 536 |           "command": "common.visit"
 537 |         },
 538 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 539 |       },
 540 |       {
 541 |         "key": "delete",
 542 |         "command": "magit.invoke",
 543 |         "args": {
 544 |           "command": "status.discard"
 545 |         },
 546 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 547 |       },
 548 |       {
 549 |         "key": "v",
 550 |         "command": "magit.invoke",
 551 |         "args": {
 552 |           "command": "status.reverse"
 553 |         },
 554 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 555 |       },
 556 |       {
 557 |         "key": "a",
 558 |         "command": "magit.invoke",
 559 |         "args": {
 560 |           "command": "status.apply"
 561 |         },
 562 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 563 |       },
 564 |       {
 565 |         "key": "shift+/",
 566 |         "command": "magit.invoke",
 567 |         "args": {
 568 |           "command": "open.dispatch"
 569 |         },
 570 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 571 |       },
 572 |       {
 573 |         "key": "h",
 574 |         "command": "magit.invoke",
 575 |         "args": {
 576 |           "command": "open.dispatch"
 577 |         },
 578 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 579 |       },
 580 |       {
 581 |         "key": "c",
 582 |         "command": "magit.invoke",
 583 |         "args": {
 584 |           "command": "open.commit"
 585 |         },
 586 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 587 |       },
 588 |       {
 589 |         "key": "b",
 590 |         "command": "magit.invoke",
 591 |         "args": {
 592 |           "command": "open.branch"
 593 |         },
 594 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 595 |       },
 596 |       {
 597 |         "key": "l",
 598 |         "command": "magit.invoke",
 599 |         "args": {
 600 |           "command": "open.log"
 601 |         },
 602 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 603 |       },
 604 |       {
 605 |         "key": "d",
 606 |         "command": "magit.invoke",
 607 |         "args": {
 608 |           "command": "open.diff"
 609 |         },
 610 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 611 |       },
 612 |       {
 613 |         "key": "f",
 614 |         "command": "magit.invoke",
 615 |         "args": {
 616 |           "command": "open.fetch"
 617 |         },
 618 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 619 |       },
 620 |       {
 621 |         "key": "shift+f",
 622 |         "command": "magit.invoke",
 623 |         "args": {
 624 |           "command": "open.pull"
 625 |         },
 626 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 627 |       },
 628 |       {
 629 |         "key": "shift+p",
 630 |         "command": "magit.invoke",
 631 |         "args": {
 632 |           "command": "open.push"
 633 |         },
 634 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 635 |       },
 636 |       {
 637 |         "key": "m",
 638 |         "command": "magit.invoke",
 639 |         "args": {
 640 |           "command": "open.merge"
 641 |         },
 642 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 643 |       },
 644 |       {
 645 |         "key": "r",
 646 |         "command": "magit.invoke",
 647 |         "args": {
 648 |           "command": "open.rebase"
 649 |         },
 650 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 651 |       },
 652 |       {
 653 |         "key": "z",
 654 |         "command": "magit.invoke",
 655 |         "args": {
 656 |           "command": "open.stash"
 657 |         },
 658 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 659 |       },
 660 |       {
 661 |         "key": "t",
 662 |         "command": "magit.invoke",
 663 |         "args": {
 664 |           "command": "open.tag"
 665 |         },
 666 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 667 |       },
 668 |       {
 669 |         "key": "x",
 670 |         "command": "magit.invoke",
 671 |         "args": {
 672 |           "command": "open.reset"
 673 |         },
 674 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 675 |       },
 676 |       {
 677 |         "key": "shift+v",
 678 |         "command": "magit.invoke",
 679 |         "args": {
 680 |           "command": "open.revert"
 681 |         },
 682 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 683 |       },
 684 |       {
 685 |         "key": "shift+a",
 686 |         "command": "magit.invoke",
 687 |         "args": {
 688 |           "command": "open.cherry-pick"
 689 |         },
 690 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 691 |       },
 692 |       {
 693 |         "key": "shift+b",
 694 |         "command": "magit.invoke",
 695 |         "args": {
 696 |           "command": "open.bisect"
 697 |         },
 698 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 699 |       },
 700 |       {
 701 |         "key": "shift+m",
 702 |         "command": "magit.invoke",
 703 |         "args": {
 704 |           "command": "open.remote"
 705 |         },
 706 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 707 |       },
 708 |       {
 709 |         "key": "o",
 710 |         "command": "magit.invoke",
 711 |         "args": {
 712 |           "command": "open.submodule"
 713 |         },
 714 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 715 |       },
 716 |       {
 717 |         "key": "shift+5",
 718 |         "command": "magit.invoke",
 719 |         "args": {
 720 |           "command": "open.worktree"
 721 |         },
 722 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 723 |       },
 724 |       {
 725 |         "key": "i",
 726 |         "command": "magit.invoke",
 727 |         "args": {
 728 |           "command": "open.ignore"
 729 |         },
 730 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 731 |       },
 732 |       {
 733 |         "key": "w",
 734 |         "command": "magit.invoke",
 735 |         "args": {
 736 |           "command": "open.patch-apply"
 737 |         },
 738 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 739 |       },
 740 |       {
 741 |         "key": "shift+w",
 742 |         "command": "magit.invoke",
 743 |         "args": {
 744 |           "command": "open.patch-create"
 745 |         },
 746 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 747 |       },
 748 |       {
 749 |         "key": "y",
 750 |         "command": "magit.invoke",
 751 |         "args": {
 752 |           "command": "open.refs"
 753 |         },
 754 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 755 |       },
 756 |       {
 757 |         "key": "shift+y",
 758 |         "command": "magit.invoke",
 759 |         "args": {
 760 |           "command": "open.cherries"
 761 |         },
 762 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 763 |       },
 764 |       {
 765 |         "key": "shift+1",
 766 |         "command": "magit.invoke",
 767 |         "args": {
 768 |           "command": "open.run"
 769 |         },
 770 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 771 |       },
 772 |       {
 773 |         "key": "shift+4",
 774 |         "command": "magit.invoke",
 775 |         "args": {
 776 |           "command": "open.process-log"
 777 |         },
 778 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'status' && !magit.textMode"
 779 |       },
 780 |       {
 781 |         "key": "f5",
 782 |         "command": "magit.invoke",
 783 |         "args": {
 784 |           "command": "common.refresh"
 785 |         },
 786 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 787 |       },
 788 |       {
 789 |         "key": "g",
 790 |         "command": "magit.invoke",
 791 |         "args": {
 792 |           "command": "common.refresh"
 793 |         },
 794 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 795 |       },
 796 |       {
 797 |         "key": "tab",
 798 |         "command": "magit.invoke",
 799 |         "args": {
 800 |           "command": "common.toggle-section"
 801 |         },
 802 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 803 |       },
 804 |       {
 805 |         "key": "alt+1",
 806 |         "command": "magit.invoke",
 807 |         "args": {
 808 |           "command": "common.detail-level-1"
 809 |         },
 810 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 811 |       },
 812 |       {
 813 |         "key": "alt+2",
 814 |         "command": "magit.invoke",
 815 |         "args": {
 816 |           "command": "common.detail-level-2"
 817 |         },
 818 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 819 |       },
 820 |       {
 821 |         "key": "alt+3",
 822 |         "command": "magit.invoke",
 823 |         "args": {
 824 |           "command": "common.detail-level-3"
 825 |         },
 826 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 827 |       },
 828 |       {
 829 |         "key": "alt+4",
 830 |         "command": "magit.invoke",
 831 |         "args": {
 832 |           "command": "common.detail-level-4"
 833 |         },
 834 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 835 |       },
 836 |       {
 837 |         "key": "escape",
 838 |         "command": "magit.invoke",
 839 |         "args": {
 840 |           "command": "common.cancel"
 841 |         },
 842 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 843 |       },
 844 |       {
 845 |         "key": "ctrl+c",
 846 |         "command": "magit.invoke",
 847 |         "args": {
 848 |           "command": "common.cancel"
 849 |         },
 850 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 851 |       },
 852 |       {
 853 |         "key": "enter",
 854 |         "command": "magit.invoke",
 855 |         "args": {
 856 |           "command": "common.confirm"
 857 |         },
 858 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 859 |       },
 860 |       {
 861 |         "key": "q",
 862 |         "command": "magit.invoke",
 863 |         "args": {
 864 |           "command": "common.quit"
 865 |         },
 866 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 867 |       },
 868 |       {
 869 |         "key": "shift+tab",
 870 |         "command": "magit.invoke",
 871 |         "args": {
 872 |           "command": "common.cycle-visibility"
 873 |         },
 874 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 875 |       },
 876 |       {
 877 |         "key": "alt+down",
 878 |         "command": "magit.invoke",
 879 |         "args": {
 880 |           "command": "common.next-sibling"
 881 |         },
 882 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 883 |       },
 884 |       {
 885 |         "key": "alt+up",
 886 |         "command": "magit.invoke",
 887 |         "args": {
 888 |           "command": "common.prev-sibling"
 889 |         },
 890 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 891 |       },
 892 |       {
 893 |         "key": "alt+left",
 894 |         "command": "magit.invoke",
 895 |         "args": {
 896 |           "command": "common.parent-section"
 897 |         },
 898 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 899 |       },
 900 |       {
 901 |         "key": "ctrl+t",
 902 |         "command": "magit.invoke",
 903 |         "args": {
 904 |           "command": "common.toggle-text-mode"
 905 |         },
 906 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 907 |       },
 908 |       {
 909 |         "key": "ctrl+t",
 910 |         "command": "magit.invoke",
 911 |         "args": {
 912 |           "command": "common.toggle-text-mode"
 913 |         },
 914 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && magit.textMode"
 915 |       },
 916 |       {
 917 |         "key": "ctrl+shift+c",
 918 |         "command": "magit.invoke",
 919 |         "args": {
 920 |           "command": "common.copy-value"
 921 |         },
 922 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 923 |       },
 924 |       {
 925 |         "key": "shift+=",
 926 |         "command": "magit.invoke",
 927 |         "args": {
 928 |           "command": "log.load-more"
 929 |         },
 930 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log' && !magit.textMode"
 931 |       },
 932 |       {
 933 |         "key": "f5",
 934 |         "command": "magit.invoke",
 935 |         "args": {
 936 |           "command": "common.refresh"
 937 |         },
 938 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 939 |       },
 940 |       {
 941 |         "key": "g",
 942 |         "command": "magit.invoke",
 943 |         "args": {
 944 |           "command": "common.refresh"
 945 |         },
 946 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 947 |       },
 948 |       {
 949 |         "key": "tab",
 950 |         "command": "magit.invoke",
 951 |         "args": {
 952 |           "command": "common.toggle-section"
 953 |         },
 954 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 955 |       },
 956 |       {
 957 |         "key": "alt+1",
 958 |         "command": "magit.invoke",
 959 |         "args": {
 960 |           "command": "common.detail-level-1"
 961 |         },
 962 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 963 |       },
 964 |       {
 965 |         "key": "alt+2",
 966 |         "command": "magit.invoke",
 967 |         "args": {
 968 |           "command": "common.detail-level-2"
 969 |         },
 970 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 971 |       },
 972 |       {
 973 |         "key": "alt+3",
 974 |         "command": "magit.invoke",
 975 |         "args": {
 976 |           "command": "common.detail-level-3"
 977 |         },
 978 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 979 |       },
 980 |       {
 981 |         "key": "alt+4",
 982 |         "command": "magit.invoke",
 983 |         "args": {
 984 |           "command": "common.detail-level-4"
 985 |         },
 986 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 987 |       },
 988 |       {
 989 |         "key": "escape",
 990 |         "command": "magit.invoke",
 991 |         "args": {
 992 |           "command": "common.cancel"
 993 |         },
 994 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
 995 |       },
 996 |       {
 997 |         "key": "ctrl+c",
 998 |         "command": "magit.invoke",
 999 |         "args": {
1000 |           "command": "common.cancel"
1001 |         },
1002 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1003 |       },
1004 |       {
1005 |         "key": "enter",
1006 |         "command": "magit.invoke",
1007 |         "args": {
1008 |           "command": "common.confirm"
1009 |         },
1010 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1011 |       },
1012 |       {
1013 |         "key": "q",
1014 |         "command": "magit.invoke",
1015 |         "args": {
1016 |           "command": "common.quit"
1017 |         },
1018 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1019 |       },
1020 |       {
1021 |         "key": "shift+tab",
1022 |         "command": "magit.invoke",
1023 |         "args": {
1024 |           "command": "common.cycle-visibility"
1025 |         },
1026 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1027 |       },
1028 |       {
1029 |         "key": "alt+down",
1030 |         "command": "magit.invoke",
1031 |         "args": {
1032 |           "command": "common.next-sibling"
1033 |         },
1034 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1035 |       },
1036 |       {
1037 |         "key": "alt+up",
1038 |         "command": "magit.invoke",
1039 |         "args": {
1040 |           "command": "common.prev-sibling"
1041 |         },
1042 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1043 |       },
1044 |       {
1045 |         "key": "alt+left",
1046 |         "command": "magit.invoke",
1047 |         "args": {
1048 |           "command": "common.parent-section"
1049 |         },
1050 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1051 |       },
1052 |       {
1053 |         "key": "ctrl+t",
1054 |         "command": "magit.invoke",
1055 |         "args": {
1056 |           "command": "common.toggle-text-mode"
1057 |         },
1058 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1059 |       },
1060 |       {
1061 |         "key": "ctrl+t",
1062 |         "command": "magit.invoke",
1063 |         "args": {
1064 |           "command": "common.toggle-text-mode"
1065 |         },
1066 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && magit.textMode"
1067 |       },
1068 |       {
1069 |         "key": "ctrl+shift+c",
1070 |         "command": "magit.invoke",
1071 |         "args": {
1072 |           "command": "common.copy-value"
1073 |         },
1074 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1075 |       },
1076 |       {
1077 |         "key": "shift+=",
1078 |         "command": "magit.invoke",
1079 |         "args": {
1080 |           "command": "log.load-more"
1081 |         },
1082 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1083 |       },
1084 |       {
1085 |         "key": "space",
1086 |         "command": "magit.invoke",
1087 |         "args": {
1088 |           "command": "select.toggle-mark"
1089 |         },
1090 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1091 |       },
1092 |       {
1093 |         "key": "i",
1094 |         "command": "magit.invoke",
1095 |         "args": {
1096 |           "command": "select.enter-rev"
1097 |         },
1098 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'log.select' && !magit.textMode"
1099 |       },
1100 |       {
1101 |         "key": "f5",
1102 |         "command": "magit.invoke",
1103 |         "args": {
1104 |           "command": "common.refresh"
1105 |         },
1106 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1107 |       },
1108 |       {
1109 |         "key": "g",
1110 |         "command": "magit.invoke",
1111 |         "args": {
1112 |           "command": "common.refresh"
1113 |         },
1114 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1115 |       },
1116 |       {
1117 |         "key": "tab",
1118 |         "command": "magit.invoke",
1119 |         "args": {
1120 |           "command": "common.toggle-section"
1121 |         },
1122 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1123 |       },
1124 |       {
1125 |         "key": "alt+1",
1126 |         "command": "magit.invoke",
1127 |         "args": {
1128 |           "command": "common.detail-level-1"
1129 |         },
1130 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1131 |       },
1132 |       {
1133 |         "key": "alt+2",
1134 |         "command": "magit.invoke",
1135 |         "args": {
1136 |           "command": "common.detail-level-2"
1137 |         },
1138 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1139 |       },
1140 |       {
1141 |         "key": "alt+3",
1142 |         "command": "magit.invoke",
1143 |         "args": {
1144 |           "command": "common.detail-level-3"
1145 |         },
1146 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1147 |       },
1148 |       {
1149 |         "key": "alt+4",
1150 |         "command": "magit.invoke",
1151 |         "args": {
1152 |           "command": "common.detail-level-4"
1153 |         },
1154 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1155 |       },
1156 |       {
1157 |         "key": "n",
1158 |         "command": "magit.invoke",
1159 |         "args": {
1160 |           "command": "common.next-section"
1161 |         },
1162 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1163 |       },
1164 |       {
1165 |         "key": "p",
1166 |         "command": "magit.invoke",
1167 |         "args": {
1168 |           "command": "common.prev-section"
1169 |         },
1170 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1171 |       },
1172 |       {
1173 |         "key": "escape",
1174 |         "command": "magit.invoke",
1175 |         "args": {
1176 |           "command": "common.cancel"
1177 |         },
1178 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1179 |       },
1180 |       {
1181 |         "key": "ctrl+c",
1182 |         "command": "magit.invoke",
1183 |         "args": {
1184 |           "command": "common.cancel"
1185 |         },
1186 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1187 |       },
1188 |       {
1189 |         "key": "enter",
1190 |         "command": "magit.invoke",
1191 |         "args": {
1192 |           "command": "common.confirm"
1193 |         },
1194 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1195 |       },
1196 |       {
1197 |         "key": "q",
1198 |         "command": "magit.invoke",
1199 |         "args": {
1200 |           "command": "common.quit"
1201 |         },
1202 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1203 |       },
1204 |       {
1205 |         "key": "shift+tab",
1206 |         "command": "magit.invoke",
1207 |         "args": {
1208 |           "command": "common.cycle-visibility"
1209 |         },
1210 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1211 |       },
1212 |       {
1213 |         "key": "alt+down",
1214 |         "command": "magit.invoke",
1215 |         "args": {
1216 |           "command": "common.next-sibling"
1217 |         },
1218 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1219 |       },
1220 |       {
1221 |         "key": "alt+up",
1222 |         "command": "magit.invoke",
1223 |         "args": {
1224 |           "command": "common.prev-sibling"
1225 |         },
1226 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1227 |       },
1228 |       {
1229 |         "key": "alt+left",
1230 |         "command": "magit.invoke",
1231 |         "args": {
1232 |           "command": "common.parent-section"
1233 |         },
1234 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1235 |       },
1236 |       {
1237 |         "key": "ctrl+t",
1238 |         "command": "magit.invoke",
1239 |         "args": {
1240 |           "command": "common.toggle-text-mode"
1241 |         },
1242 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1243 |       },
1244 |       {
1245 |         "key": "ctrl+t",
1246 |         "command": "magit.invoke",
1247 |         "args": {
1248 |           "command": "common.toggle-text-mode"
1249 |         },
1250 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && magit.textMode"
1251 |       },
1252 |       {
1253 |         "key": "ctrl+shift+c",
1254 |         "command": "magit.invoke",
1255 |         "args": {
1256 |           "command": "common.copy-value"
1257 |         },
1258 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1259 |       },
1260 |       {
1261 |         "key": "shift+/",
1262 |         "command": "magit.invoke",
1263 |         "args": {
1264 |           "command": "open.dispatch"
1265 |         },
1266 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1267 |       },
1268 |       {
1269 |         "key": "space",
1270 |         "command": "magit.invoke",
1271 |         "args": {
1272 |           "command": "select.toggle-mark"
1273 |         },
1274 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1275 |       },
1276 |       {
1277 |         "key": "i",
1278 |         "command": "magit.invoke",
1279 |         "args": {
1280 |           "command": "select.enter-rev"
1281 |         },
1282 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'select' && !magit.textMode"
1283 |       },
1284 |       {
1285 |         "key": "f5",
1286 |         "command": "magit.invoke",
1287 |         "args": {
1288 |           "command": "common.refresh"
1289 |         },
1290 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1291 |       },
1292 |       {
1293 |         "key": "g",
1294 |         "command": "magit.invoke",
1295 |         "args": {
1296 |           "command": "common.refresh"
1297 |         },
1298 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1299 |       },
1300 |       {
1301 |         "key": "tab",
1302 |         "command": "magit.invoke",
1303 |         "args": {
1304 |           "command": "common.toggle-section"
1305 |         },
1306 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1307 |       },
1308 |       {
1309 |         "key": "alt+1",
1310 |         "command": "magit.invoke",
1311 |         "args": {
1312 |           "command": "common.detail-level-1"
1313 |         },
1314 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1315 |       },
1316 |       {
1317 |         "key": "alt+2",
1318 |         "command": "magit.invoke",
1319 |         "args": {
1320 |           "command": "common.detail-level-2"
1321 |         },
1322 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1323 |       },
1324 |       {
1325 |         "key": "alt+3",
1326 |         "command": "magit.invoke",
1327 |         "args": {
1328 |           "command": "common.detail-level-3"
1329 |         },
1330 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1331 |       },
1332 |       {
1333 |         "key": "alt+4",
1334 |         "command": "magit.invoke",
1335 |         "args": {
1336 |           "command": "common.detail-level-4"
1337 |         },
1338 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1339 |       },
1340 |       {
1341 |         "key": "n",
1342 |         "command": "magit.invoke",
1343 |         "args": {
1344 |           "command": "common.next-section"
1345 |         },
1346 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1347 |       },
1348 |       {
1349 |         "key": "p",
1350 |         "command": "magit.invoke",
1351 |         "args": {
1352 |           "command": "common.prev-section"
1353 |         },
1354 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1355 |       },
1356 |       {
1357 |         "key": "escape",
1358 |         "command": "magit.invoke",
1359 |         "args": {
1360 |           "command": "common.cancel"
1361 |         },
1362 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1363 |       },
1364 |       {
1365 |         "key": "ctrl+c",
1366 |         "command": "magit.invoke",
1367 |         "args": {
1368 |           "command": "common.cancel"
1369 |         },
1370 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1371 |       },
1372 |       {
1373 |         "key": "enter",
1374 |         "command": "magit.invoke",
1375 |         "args": {
1376 |           "command": "common.confirm"
1377 |         },
1378 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1379 |       },
1380 |       {
1381 |         "key": "q",
1382 |         "command": "magit.invoke",
1383 |         "args": {
1384 |           "command": "common.quit"
1385 |         },
1386 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1387 |       },
1388 |       {
1389 |         "key": "shift+tab",
1390 |         "command": "magit.invoke",
1391 |         "args": {
1392 |           "command": "common.cycle-visibility"
1393 |         },
1394 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1395 |       },
1396 |       {
1397 |         "key": "alt+down",
1398 |         "command": "magit.invoke",
1399 |         "args": {
1400 |           "command": "common.next-sibling"
1401 |         },
1402 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1403 |       },
1404 |       {
1405 |         "key": "alt+up",
1406 |         "command": "magit.invoke",
1407 |         "args": {
1408 |           "command": "common.prev-sibling"
1409 |         },
1410 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1411 |       },
1412 |       {
1413 |         "key": "alt+left",
1414 |         "command": "magit.invoke",
1415 |         "args": {
1416 |           "command": "common.parent-section"
1417 |         },
1418 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1419 |       },
1420 |       {
1421 |         "key": "ctrl+t",
1422 |         "command": "magit.invoke",
1423 |         "args": {
1424 |           "command": "common.toggle-text-mode"
1425 |         },
1426 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1427 |       },
1428 |       {
1429 |         "key": "ctrl+t",
1430 |         "command": "magit.invoke",
1431 |         "args": {
1432 |           "command": "common.toggle-text-mode"
1433 |         },
1434 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && magit.textMode"
1435 |       },
1436 |       {
1437 |         "key": "ctrl+shift+c",
1438 |         "command": "magit.invoke",
1439 |         "args": {
1440 |           "command": "common.copy-value"
1441 |         },
1442 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1443 |       },
1444 |       {
1445 |         "key": "shift+/",
1446 |         "command": "magit.invoke",
1447 |         "args": {
1448 |           "command": "open.dispatch"
1449 |         },
1450 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1451 |       },
1452 |       {
1453 |         "key": "ctrl+s",
1454 |         "command": "magit.invoke",
1455 |         "args": {
1456 |           "command": "menu.save-args"
1457 |         },
1458 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1459 |       },
1460 |       {
1461 |         "key": "ctrl+r",
1462 |         "command": "magit.invoke",
1463 |         "args": {
1464 |           "command": "menu.reset-args"
1465 |         },
1466 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'menu' && !magit.textMode"
1467 |       },
1468 |       {
1469 |         "key": "f5",
1470 |         "command": "magit.invoke",
1471 |         "args": {
1472 |           "command": "common.refresh"
1473 |         },
1474 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1475 |       },
1476 |       {
1477 |         "key": "g",
1478 |         "command": "magit.invoke",
1479 |         "args": {
1480 |           "command": "common.refresh"
1481 |         },
1482 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1483 |       },
1484 |       {
1485 |         "key": "tab",
1486 |         "command": "magit.invoke",
1487 |         "args": {
1488 |           "command": "common.toggle-section"
1489 |         },
1490 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1491 |       },
1492 |       {
1493 |         "key": "alt+1",
1494 |         "command": "magit.invoke",
1495 |         "args": {
1496 |           "command": "common.detail-level-1"
1497 |         },
1498 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1499 |       },
1500 |       {
1501 |         "key": "alt+2",
1502 |         "command": "magit.invoke",
1503 |         "args": {
1504 |           "command": "common.detail-level-2"
1505 |         },
1506 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1507 |       },
1508 |       {
1509 |         "key": "alt+3",
1510 |         "command": "magit.invoke",
1511 |         "args": {
1512 |           "command": "common.detail-level-3"
1513 |         },
1514 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1515 |       },
1516 |       {
1517 |         "key": "alt+4",
1518 |         "command": "magit.invoke",
1519 |         "args": {
1520 |           "command": "common.detail-level-4"
1521 |         },
1522 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1523 |       },
1524 |       {
1525 |         "key": "n",
1526 |         "command": "magit.invoke",
1527 |         "args": {
1528 |           "command": "common.next-section"
1529 |         },
1530 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1531 |       },
1532 |       {
1533 |         "key": "p",
1534 |         "command": "magit.invoke",
1535 |         "args": {
1536 |           "command": "common.prev-section"
1537 |         },
1538 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1539 |       },
1540 |       {
1541 |         "key": "ctrl+shift+x",
1542 |         "command": "magit.invoke",
1543 |         "args": {
1544 |           "command": "common.cancel"
1545 |         },
1546 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1547 |       },
1548 |       {
1549 |         "key": "ctrl+enter",
1550 |         "command": "magit.invoke",
1551 |         "args": {
1552 |           "command": "common.confirm"
1553 |         },
1554 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1555 |       },
1556 |       {
1557 |         "key": "q",
1558 |         "command": "magit.invoke",
1559 |         "args": {
1560 |           "command": "common.quit"
1561 |         },
1562 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1563 |       },
1564 |       {
1565 |         "key": "shift+tab",
1566 |         "command": "magit.invoke",
1567 |         "args": {
1568 |           "command": "common.cycle-visibility"
1569 |         },
1570 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1571 |       },
1572 |       {
1573 |         "key": "alt+left",
1574 |         "command": "magit.invoke",
1575 |         "args": {
1576 |           "command": "common.parent-section"
1577 |         },
1578 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1579 |       },
1580 |       {
1581 |         "key": "ctrl+t",
1582 |         "command": "magit.invoke",
1583 |         "args": {
1584 |           "command": "common.toggle-text-mode"
1585 |         },
1586 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1587 |       },
1588 |       {
1589 |         "key": "ctrl+t",
1590 |         "command": "magit.invoke",
1591 |         "args": {
1592 |           "command": "common.toggle-text-mode"
1593 |         },
1594 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && magit.textMode"
1595 |       },
1596 |       {
1597 |         "key": "ctrl+shift+c",
1598 |         "command": "magit.invoke",
1599 |         "args": {
1600 |           "command": "common.copy-value"
1601 |         },
1602 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1603 |       },
1604 |       {
1605 |         "key": "shift+/",
1606 |         "command": "magit.invoke",
1607 |         "args": {
1608 |           "command": "open.dispatch"
1609 |         },
1610 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1611 |       },
1612 |       {
1613 |         "key": "alt+up",
1614 |         "command": "magit.invoke",
1615 |         "args": {
1616 |           "command": "commit-message.history-prev"
1617 |         },
1618 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1619 |       },
1620 |       {
1621 |         "key": "alt+down",
1622 |         "command": "magit.invoke",
1623 |         "args": {
1624 |           "command": "commit-message.history-next"
1625 |         },
1626 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1627 |       },
1628 |       {
1629 |         "key": "ctrl+alt+s",
1630 |         "command": "magit.invoke",
1631 |         "args": {
1632 |           "command": "commit-message.add-signoff"
1633 |         },
1634 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'commit-message' && !magit.textMode"
1635 |       },
1636 |       {
1637 |         "key": "f5",
1638 |         "command": "magit.invoke",
1639 |         "args": {
1640 |           "command": "common.refresh"
1641 |         },
1642 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1643 |       },
1644 |       {
1645 |         "key": "g",
1646 |         "command": "magit.invoke",
1647 |         "args": {
1648 |           "command": "common.refresh"
1649 |         },
1650 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1651 |       },
1652 |       {
1653 |         "key": "tab",
1654 |         "command": "magit.invoke",
1655 |         "args": {
1656 |           "command": "common.toggle-section"
1657 |         },
1658 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1659 |       },
1660 |       {
1661 |         "key": "alt+1",
1662 |         "command": "magit.invoke",
1663 |         "args": {
1664 |           "command": "common.detail-level-1"
1665 |         },
1666 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1667 |       },
1668 |       {
1669 |         "key": "alt+2",
1670 |         "command": "magit.invoke",
1671 |         "args": {
1672 |           "command": "common.detail-level-2"
1673 |         },
1674 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1675 |       },
1676 |       {
1677 |         "key": "alt+3",
1678 |         "command": "magit.invoke",
1679 |         "args": {
1680 |           "command": "common.detail-level-3"
1681 |         },
1682 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1683 |       },
1684 |       {
1685 |         "key": "alt+4",
1686 |         "command": "magit.invoke",
1687 |         "args": {
1688 |           "command": "common.detail-level-4"
1689 |         },
1690 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1691 |       },
1692 |       {
1693 |         "key": "n",
1694 |         "command": "magit.invoke",
1695 |         "args": {
1696 |           "command": "common.next-section"
1697 |         },
1698 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1699 |       },
1700 |       {
1701 |         "key": "p",
1702 |         "command": "magit.invoke",
1703 |         "args": {
1704 |           "command": "common.prev-section"
1705 |         },
1706 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1707 |       },
1708 |       {
1709 |         "key": "escape",
1710 |         "command": "magit.invoke",
1711 |         "args": {
1712 |           "command": "common.cancel"
1713 |         },
1714 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1715 |       },
1716 |       {
1717 |         "key": "ctrl+c",
1718 |         "command": "magit.invoke",
1719 |         "args": {
1720 |           "command": "common.cancel"
1721 |         },
1722 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1723 |       },
1724 |       {
1725 |         "key": "enter",
1726 |         "command": "magit.invoke",
1727 |         "args": {
1728 |           "command": "common.confirm"
1729 |         },
1730 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1731 |       },
1732 |       {
1733 |         "key": "q",
1734 |         "command": "magit.invoke",
1735 |         "args": {
1736 |           "command": "common.quit"
1737 |         },
1738 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1739 |       },
1740 |       {
1741 |         "key": "shift+tab",
1742 |         "command": "magit.invoke",
1743 |         "args": {
1744 |           "command": "common.cycle-visibility"
1745 |         },
1746 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1747 |       },
1748 |       {
1749 |         "key": "alt+down",
1750 |         "command": "magit.invoke",
1751 |         "args": {
1752 |           "command": "common.next-sibling"
1753 |         },
1754 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1755 |       },
1756 |       {
1757 |         "key": "alt+up",
1758 |         "command": "magit.invoke",
1759 |         "args": {
1760 |           "command": "common.prev-sibling"
1761 |         },
1762 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1763 |       },
1764 |       {
1765 |         "key": "alt+left",
1766 |         "command": "magit.invoke",
1767 |         "args": {
1768 |           "command": "common.parent-section"
1769 |         },
1770 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1771 |       },
1772 |       {
1773 |         "key": "ctrl+t",
1774 |         "command": "magit.invoke",
1775 |         "args": {
1776 |           "command": "common.toggle-text-mode"
1777 |         },
1778 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1779 |       },
1780 |       {
1781 |         "key": "ctrl+t",
1782 |         "command": "magit.invoke",
1783 |         "args": {
1784 |           "command": "common.toggle-text-mode"
1785 |         },
1786 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && magit.textMode"
1787 |       },
1788 |       {
1789 |         "key": "ctrl+shift+c",
1790 |         "command": "magit.invoke",
1791 |         "args": {
1792 |           "command": "common.copy-value"
1793 |         },
1794 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1795 |       },
1796 |       {
1797 |         "key": "shift+/",
1798 |         "command": "magit.invoke",
1799 |         "args": {
1800 |           "command": "open.dispatch"
1801 |         },
1802 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1803 |       },
1804 |       {
1805 |         "key": "c",
1806 |         "command": "magit.invoke",
1807 |         "args": {
1808 |           "command": "process-log.clear"
1809 |         },
1810 |         "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == 'process-log' && !magit.textMode"
1811 |       }
1812 |     ],
1813 |     "configuration": {
1814 |       "title": "magit-lsp",
1815 |       "properties": {
1816 |         "magit-lsp.serverPath": {
1817 |           "type": "string",
1818 |           "default": "magit-lsp",
1819 |           "description": "Path to the magit-lsp executable."
1820 |         },
1821 |         "magit-lsp.profile": {
1822 |           "type": "string",
1823 |           "enum": [
1824 |             "standard",
1825 |             "vim"
1826 |           ],
1827 |           "default": "standard",
1828 |           "description": "Keymap profile the server resolves."
1829 |         },
1830 |         "magit-lsp.keybindingsPath": {
1831 |           "type": "string",
1832 |           "default": "",
1833 |           "description": "Optional path to VSCode's keybindings.json for fold-shortcut mirroring."
1834 |         }
1835 |       }
1836 |     }
1837 |   }
1838 | }
```

### Client session

### `clients/vscode/lib/session.js` lines 1-147 of 147
```text
   1 | 'use strict';
   2 |
   3 | const vscode = require('vscode');
   4 | const { LanguageClient } = require('vscode-languageclient/node');
   5 |
   6 | const commands = require('./commands');
   7 | const edits = require('./edits');
   8 | const drafts = require('./drafts');
   9 | const effects = require('./effects');
  10 | const requests = require('./requests');
  11 | const semantic = require('./semantic');
  12 | const views = require('./views');
  13 | const { SCHEME, store } = require('./store');
  14 | const { message } = require('./util');
  15 |
  16 | /**
  17 |  * Create the LSP client with the static key-delivery capabilities.
  18 |  *
  19 |  * @returns {LanguageClient}
  20 |  */
  21 | function createClient() {
  22 |   const config = vscode.workspace.getConfiguration('magit-lsp');
  23 |   const command = config.get('serverPath') || 'magit-lsp';
  24 |   const profile = config.get('profile') || 'standard';
  25 |
  26 |   return new LanguageClient(
  27 |     'magit-lsp',
  28 |     'magit-lsp',
  29 |     { command, args: [] },
  30 |     {
  31 |       documentSelector: [{ scheme: SCHEME }],
  32 |       initializationOptions: { profile },
  33 |       uriConverters: {
  34 |         code2Protocol: (uri) => views.stateForUri(uri)?.descriptor.uri || uri.toString(),
  35 |         protocol2Code: (uri) => Array.from(store.views.values())
  36 |           .find((state) => state.descriptor.uri === uri)?.uri || vscode.Uri.parse(uri),
  37 |       },
  38 |       middleware: {
  39 |         // The explicit provider maps dotted server types to legal VSCode IDs.
  40 |         provideDocumentSemanticTokens: () => null,
  41 |         initialize(params, second, third) {
  42 |           params.clientInfo = { name: 'Visual Studio Code', version: vscode.version };
  43 |           params.capabilities = params.capabilities || {};
  44 |           params.capabilities.experimental = params.capabilities.experimental || {};
  45 |           params.capabilities.experimental.magit = {
  46 |             v: 1,
  47 |             keys: 'static',
  48 |             modifiers: 'full',
  49 |             render: 'edits',
  50 |             layout: ['main', 'side', 'bottom', 'float'],
  51 |             effects: ['visitFile', 'editDocument', 'clipboard'],
  52 |             prompts: ['confirm', 'text', 'choice', 'multiChoice', 'secret'],
  53 |             folds: 'server',
  54 |           };
  55 |           const next = typeof second === 'function' ? second : third;
  56 |           return next(params);
  57 |         },
  58 |       },
  59 |     }
  60 |   );
  61 | }
  62 |
  63 | /**
  64 |  * Activate the extension.
  65 |  *
  66 |  * @param {vscode.ExtensionContext} context
  67 |  */
  68 | function activate(context) {
  69 |   store.output = vscode.window.createOutputChannel('magit-lsp');
  70 |   context.subscriptions.push(store.output);
  71 |
  72 |   store.documentsChanged = new vscode.EventEmitter();
  73 |   context.subscriptions.push(store.documentsChanged);
  74 |
  75 |   context.subscriptions.push(
  76 |     vscode.workspace.registerTextDocumentContentProvider(SCHEME, {
  77 |       onDidChange: store.documentsChanged.event,
  78 |       provideTextDocumentContent(uri) {
  79 |         const state = views.stateForUri(uri);
  80 |         return state ? state.lines.join('\n') : '';
  81 |       },
  82 |     })
  83 |   );
  84 |
  85 |   context.subscriptions.push(
  86 |     vscode.commands.registerCommand('magit.open', commands.openStatus),
  87 |     vscode.commands.registerCommand('magit.key', commands.pressCommand),
  88 |      vscode.commands.registerCommand('magit.invoke', async (args) => {
  89 |        if (drafts.current()) return drafts.handleInvoke(args);
  90 |        return commands.invokeCommand(args);
  91 |      }),
  92 |     vscode.commands.registerCommand('magit.confirmEdit', edits.confirmEditCommand),
  93 |      vscode.commands.registerCommand('magit.cancelEdit', edits.cancelEditCommand),
  94 |      vscode.commands.registerCommand('magit.closeEdit', () => {
  95 |        const draft = drafts.current();
  96 |        if (draft) return drafts.close(draft);
  97 |      }),
  98 |     vscode.commands.registerCommand('magit.mirrorFoldShortcuts', commands.mirrorFoldShortcuts),
  99 |      vscode.workspace.onDidCloseTextDocument(edits.onDocumentClosed),
 100 |      vscode.workspace.onDidChangeTextDocument(drafts.onChanged),
 101 |     vscode.window.onDidChangeActiveTextEditor(edits.onActiveEditorChanged)
 102 |    );
 103 |
 104 |    drafts.setup(context);
 105 |
 106 |   store.client = createClient();
 107 |   const client = store.client;
 108 |   client.onNotification('view/render', requests.applyRender);
 109 |    client.onNotification('view/prompt', requests.handlePrompt);
 110 |    // The explicit full-sync stream excludes the draft scheme from the language
 111 |    // client's automatic document selector, avoiding duplicate native versions.
 112 |    if (client.onDidChangeState) context.subscriptions.push(client.onDidChangeState((event) => {
 113 |      if (event.newState === 1) drafts.disconnect('the server disconnected; copy your text before reopening');
 114 |    }));
 115 |   client.onNotification('view/effects', (params) => {
 116 |     void effects.applyEffects(params && params.effects, store.activeViewId);
 117 |   });
 118 |   store.clientReady = client.start().then(() => {
 119 |     store.semanticChanged = semantic.register(
 120 |       vscode,
 121 |       client,
 122 |       views.stateForUri,
 123 |       context.subscriptions
 124 |     );
 125 |   }).catch((error) => {
 126 |     store.output.appendLine(`language client failed: ${message(error)}`);
 127 |     vscode.window.showErrorMessage(`magit-lsp: ${message(error)}`);
 128 |   });
 129 |
 130 |   views.activateView(undefined);
 131 | }
 132 |
 133 | /**
 134 |  * Stop the language client when the extension deactivates.
 135 |  */
 136 | async function deactivate() {
 137 |   store.disposing = true;
 138 |   drafts.disconnect('the editor session is closing; copy your text before reopening');
 139 |   if (store.client) {
 140 |     await store.client.stop();
 141 |     store.client = undefined;
 142 |   }
 143 | }
 144 |
 145 | exports.activate = activate;
 146 | exports.deactivate = deactivate;
 147 | exports.createClient = createClient;
```
### Fake LanguageClient implementation — full source

### `clients/vscode/test/fake/languageclient.js` lines 1-234 of 234
```text
   1 | 'use strict';
   2 |
   3 | /**
   4 |  * A minimal `LanguageClient` replacement that speaks LSP over stdio.
   5 |  *
   6 |  * The node integration test needs the extension to run against the real
   7 |  * `magit-lsp` binary without VSCode or `vscode-languageclient`'s editor
   8 |  * services. This client implements the JSON-RPC framing, the initialize
   9 |  * handshake and the request/notification surface the extension uses. It
  10 |  * records every outgoing message so tests can assert what the client sent.
  11 |  */
  12 |
  13 | const { spawn } = require('node:child_process');
  14 |
  15 | /** Every client this process constructed, for tests to inspect. */
  16 | const instances = [];
  17 |
  18 | /** One outgoing message, recorded for assertions. */
  19 | class Recording {
  20 |   constructor(method, params) {
  21 |     this.method = method;
  22 |     this.params = params;
  23 |   }
  24 | }
  25 |
  26 | /** A JSON-RPC language client over a child process's stdio. */
  27 | class LanguageClient {
  28 |   /**
  29 |    * @param {string} id
  30 |    * @param {string} name
  31 |    * @param {{command: string, args?: string[]}} serverOptions
  32 |    * @param {object} clientOptions
  33 |    */
  34 |   constructor(id, name, serverOptions, clientOptions) {
  35 |     this.id = id;
  36 |     this.name = name;
  37 |     this.serverOptions = serverOptions;
  38 |     this.clientOptions = clientOptions;
  39 |     this.notifications = new Map();
  40 |     this.pending = new Map();
  41 |     this.nextId = 1;
  42 |     this.buffer = Buffer.alloc(0);
  43 |     this.requests = [];
  44 |     this.notes = [];
  45 |     this.initializeResult = undefined;
  46 |     this.stderr = '';
  47 |     this.exited = undefined;
  48 |     instances.push(this);
  49 |   }
  50 |
  51 |   /**
  52 |    * Register a handler for a server notification.
  53 |    *
  54 |    * @param {string} method
  55 |    * @param {(params: unknown) => void} handler
  56 |    */
  57 |   onNotification(method, handler) {
  58 |     this.notifications.set(method, handler);
  59 |   }
  60 |
  61 |   /**
  62 |    * Start the server, run the LSP initialize handshake and resolve.
  63 |    *
  64 |    * @returns {Promise<void>}
  65 |    */
  66 |   async start() {
  67 |     this.process = spawn(this.serverOptions.command, this.serverOptions.args || [], {
  68 |       stdio: ['pipe', 'pipe', 'pipe'],
  69 |     });
  70 |     this.exited = new Promise((resolve) => {
  71 |       this.process.on('exit', (code, signal) => {
  72 |         for (const { reject } of this.pending.values()) {
  73 |           reject(new Error(`magit-lsp exited (${code ?? signal})`));
  74 |         }
  75 |         this.pending.clear();
  76 |         resolve({ code, signal });
  77 |       });
  78 |     });
  79 |     this.process.stdout.on('data', (chunk) => this.receive(chunk));
  80 |     this.process.stderr.on('data', (chunk) => { this.stderr += chunk.toString(); });
  81 |
  82 |     const params = {
  83 |       processId: process.pid,
  84 |       clientInfo: { name: 'magit-lsp-node-test', version: '0.0.0' },
  85 |       rootUri: this.clientOptions.rootUri || null,
  86 |       capabilities: {},
  87 |       initializationOptions: this.clientOptions.initializationOptions,
  88 |     };
  89 |     const next = async (requestParams) => this.request('initialize', requestParams);
  90 |     const middleware = this.clientOptions.middleware || {};
  91 |     if (middleware.initialize) {
  92 |       this.initializeResult = await middleware.initialize(params, next, undefined);
  93 |     } else {
  94 |       this.initializeResult = await next(params);
  95 |     }
  96 |     this.sendNotification('initialized', {});
  97 |   }
  98 |
  99 |   /**
 100 |    * Send one request and resolve with its result.
 101 |    *
 102 |    * @param {string} method
 103 |    * @param {unknown} params
 104 |    * @returns {Promise<any>}
 105 |    */
 106 |   sendRequest(method, params) {
 107 |     return this.request(method, params);
 108 |   }
 109 |
 110 |   /**
 111 |    * Send one request, recording it for assertions first.
 112 |    *
 113 |    * @param {string} method
 114 |    * @param {unknown} params
 115 |    * @returns {Promise<any>}
 116 |    */
 117 |   request(method, params) {
 118 |     const id = this.nextId++;
 119 |     this.requests.push(new Recording(method, params));
 120 |     return new Promise((resolve, reject) => {
 121 |       this.pending.set(id, { resolve, reject });
 122 |       this.write({ jsonrpc: '2.0', id, method, params });
 123 |     });
 124 |   }
 125 |
 126 |   /**
 127 |    * Send one notification and record it.
 128 |    *
 129 |    * @param {string} method
 130 |    * @param {unknown} params
 131 |    */
 132 |   sendNotification(method, params) {
 133 |     this.notes.push(new Recording(method, params));
 134 |     this.write({ jsonrpc: '2.0', method, params });
 135 |   }
 136 |
 137 |   /**
 138 |    * Stop the server cleanly, then force it down if it lingers.
 139 |    *
 140 |    * @returns {Promise<void>}
 141 |    */
 142 |   async stop() {
 143 |     if (!this.process || this.process.exitCode !== null) {
 144 |       return;
 145 |     }
 146 |     try {
 147 |       await this.request('shutdown', null);
 148 |       this.sendNotification('exit', null);
 149 |     } catch {
 150 |       // The server may already be gone; the kill below is the fallback.
 151 |     }
 152 |     const timedOut = await Promise.race([
 153 |       this.exited.then(() => false),
 154 |       new Promise((resolve) => setTimeout(() => resolve(true), 5000)),
 155 |     ]);
 156 |     if (timedOut) {
 157 |       this.process.kill('SIGKILL');
 158 |       await this.exited;
 159 |     }
 160 |   }
 161 |
 162 |   /**
 163 |    * Frame and write one JSON-RPC message.
 164 |    *
 165 |    * @param {object} message
 166 |    */
 167 |   write(message) {
 168 |     const body = Buffer.from(JSON.stringify(message), 'utf8');
 169 |     this.process.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
 170 |     this.process.stdin.write(body);
 171 |   }
 172 |
 173 |   /**
 174 |    * Consume an arbitrary stdout chunk, keeping incomplete frames buffered.
 175 |    *
 176 |    * @param {Buffer} chunk
 177 |    */
 178 |   receive(chunk) {
 179 |     this.buffer = Buffer.concat([this.buffer, chunk]);
 180 |     for (;;) {
 181 |       const headerEnd = this.buffer.indexOf('\r\n\r\n');
 182 |       if (headerEnd < 0) {
 183 |         return;
 184 |       }
 185 |       const header = this.buffer.subarray(0, headerEnd).toString('ascii');
 186 |       const match = /Content-Length:\s*(\d+)/i.exec(header);
 187 |       if (!match) {
 188 |         throw new Error(`malformed LSP header: ${header}`);
 189 |       }
 190 |       const length = Number(match[1]);
 191 |       const start = headerEnd + 4;
 192 |       if (this.buffer.length < start + length) {
 193 |         return;
 194 |       }
 195 |       const body = this.buffer.subarray(start, start + length).toString('utf8');
 196 |       this.buffer = this.buffer.subarray(start + length);
 197 |       this.dispatch(JSON.parse(body));
 198 |     }
 199 |   }
 200 |
 201 |   /**
 202 |    * Route one decoded message to a pending request or a notification handler.
 203 |    *
 204 |    * @param {object} message
 205 |    */
 206 |   dispatch(message) {
 207 |     if (message.id !== undefined && message.method === undefined) {
 208 |       const pending = this.pending.get(message.id);
 209 |       if (!pending) {
 210 |         return;
 211 |       }
 212 |       this.pending.delete(message.id);
 213 |       if (message.error) {
 214 |         pending.reject(new Error(message.error.message || JSON.stringify(message.error)));
 215 |       } else {
 216 |         pending.resolve(message.result);
 217 |       }
 218 |       return;
 219 |     }
 220 |     if (message.method !== undefined && message.id !== undefined) {
 221 |       // Server-originated requests are not part of this protocol; answer null.
 222 |       this.write({ jsonrpc: '2.0', id: message.id, result: null });
 223 |       return;
 224 |     }
 225 |     if (message.method !== undefined) {
 226 |       const handler = this.notifications.get(message.method);
 227 |       if (handler) {
 228 |         handler(message.params);
 229 |       }
 230 |     }
 231 |   }
 232 | }
 233 |
 234 | module.exports = { LanguageClient, instances };
```
### Fake VSCode implementation and implemented API — full source

### `clients/vscode/test/fake/vscode.js` lines 1-588 of 588
```text
   1 | 'use strict';
   2 |
   3 | /**
   4 |  * The scripted `vscode` module used by the node integration test.
   5 |  *
   6 |  * It implements only the API surface `lib/` and `extension.js` call, records
   7 |  * every widget interaction for assertions and lets the test script answers to
   8 |  * QuickPick and input prompts. Nothing here parses server text or structure.
   9 |  */
  10 |
  11 | const fs = require('node:fs');
  12 |
  13 | /** All observable fake-window state, reset between test cases. */
  14 | const state = {
  15 |   commands: new Map(),
  16 |   providers: new Map(),
  17 |   fileProviders: new Map(),
  18 |   documents: new Map(),
  19 |   editors: [],
  20 |   activeEditor: undefined,
  21 |   messages: [],
  22 |   output: [],
  23 |   contextValues: new Map(),
  24 |   executed: [],
  25 |   quickPicks: [],
  26 |   lastQuickPick: undefined,
  27 |   quickPickAnswer: undefined,
  28 |   inputAnswers: [],
  29 |   informationAnswer: undefined,
  30 |   closeHandlers: [],
  31 |   changeHandlers: [],
  32 |   activeEditorHandlers: [],
  33 |   workspaceFolder: undefined,
  34 |   configuration: {},
  35 |   clipboard: [],
  36 |   semanticProviders: [],
  37 | };
  38 |
  39 | /** A VSCode-like URI with `file`, `magit` and provider documents. */
  40 | class Uri {
  41 |   /**
  42 |    * @param {{scheme: string, authority?: string, path?: string}} parts
  43 |    */
  44 |   constructor({ scheme, authority = '', path = '' }) {
  45 |     Object.assign(this, { scheme, authority, path });
  46 |   }
  47 |
  48 |   /** @param {object} parts */
  49 |   static from(parts) {
  50 |     return new Uri(parts);
  51 |   }
  52 |
  53 |   /** @param {string} value */
  54 |   static parse(value) {
  55 |     const match = /^([^:]+):\/\/([^/]*)(\/.*)$/.exec(value);
  56 |     if (!match) {
  57 |       return new Uri({ scheme: 'file', path: value });
  58 |     }
  59 |     return new Uri({ scheme: match[1], authority: match[2], path: match[3] });
  60 |   }
  61 |
  62 |   /** @param {string} file */
  63 |   static file(file) {
  64 |     return new Uri({ scheme: 'file', path: file });
  65 |   }
  66 |
  67 |   /** The local filesystem path of a `file` URI. */
  68 |   get fsPath() {
  69 |     return this.path;
  70 |   }
  71 |
  72 |   toString() {
  73 |     return `${this.scheme}://${this.authority}${this.path}`;
  74 |   }
  75 | }
  76 |
  77 | /** A VSCode-compatible event emitter. */
  78 | class EventEmitter {
  79 |   constructor() {
  80 |     this.listeners = [];
  81 |     this.event = (listener) => {
  82 |       this.listeners.push(listener);
  83 |       return { dispose: () => {} };
  84 |     };
  85 |   }
  86 |
  87 |   /** @param {unknown} value */
  88 |   fire(value) {
  89 |     for (const listener of this.listeners) listener(value);
  90 |   }
  91 |
  92 |   dispose() {
  93 |     this.listeners = [];
  94 |   }
  95 | }
  96 |
  97 | /** A zero-based text position. */
  98 | class Position {
  99 |   /** @param {number} line @param {number} character */
 100 |   constructor(line, character) {
 101 |     this.line = line;
 102 |     this.character = character;
 103 |   }
 104 | }
 105 |
 106 | /** A selection with an anchor and an active end. */
 107 | class Selection {
 108 |   /** @param {Position} anchor @param {Position} active */
 109 |   constructor(anchor, active) {
 110 |     this.anchor = anchor;
 111 |     this.active = active;
 112 |     const ordered = (left, right) => left.line < right.line
 113 |       || (left.line === right.line && left.character <= right.character);
 114 |     this.start = ordered(anchor, active) ? anchor : active;
 115 |     this.end = ordered(anchor, active) ? active : anchor;
 116 |     this.isEmpty = anchor.line === active.line && anchor.character === active.character;
 117 |   }
 118 | }
 119 |
 120 | /** A range between two positions. */
 121 | class Range {
 122 |   /** @param {Position} start @param {Position} end */
 123 |   constructor(start, end) {
 124 |     this.start = start;
 125 |     this.end = end;
 126 |   }
 127 | }
 128 |
 129 | /** Whole-document edits used by managed-draft history. */
 130 | class WorkspaceEdit {
 131 |   constructor() { this.edits = []; }
 132 |   /** Record a native replacement. */
 133 |   replace(uri, range, text) { this.edits.push({ uri, range, text }); }
 134 | }
 135 |
 136 | /** The semantic-token legend the server declares. */
 137 | class SemanticTokensLegend {
 138 |   /** @param {string[]} tokenTypes @param {string[]} tokenModifiers */
 139 |   constructor(tokenTypes, tokenModifiers) {
 140 |     this.tokenTypes = tokenTypes;
 141 |     this.tokenModifiers = tokenModifiers;
 142 |   }
 143 | }
 144 |
 145 | /** Decoded semantic tokens. */
 146 | class SemanticTokens {
 147 |   /** @param {Uint32Array} data @param {string} [resultId] */
 148 |   constructor(data, resultId) {
 149 |     this.data = data;
 150 |     this.resultId = resultId;
 151 |   }
 152 | }
 153 |
 154 | /**
 155 |  * Read the text a provider document currently exposes.
 156 |  *
 157 |  * @param {Uri} uri
 158 |  * @returns {string}
 159 |  */
 160 | function providerText(uri) {
 161 |   const provider = state.providers.get(uri.scheme);
 162 |   return provider ? provider.provideTextDocumentContent(uri) : '';
 163 | }
 164 |
 165 | /**
 166 |  * A text document over a provider URI, a real file or in-memory content.
 167 |  *
 168 |  * @param {Uri} uri
 169 |  * @param {string} content
 170 |  * @returns {object}
 171 |  */
 172 | function makeDocument(uri, content) {
 173 |   const lines = () => content.split('\n');
 174 |   const document = {
 175 |     uri,
 176 |     version: 1,
 177 |     isDirty: false,
 178 |     isClosed: false,
 179 |     languageId: 'plaintext',
 180 |     getText: () => content,
 181 |     get lineCount() {
 182 |       return lines().length;
 183 |     },
 184 |     lineAt(line) {
 185 |       return { text: lines()[line] || '' };
 186 |     },
 187 |     async save() {
 188 |       const provider = state.fileProviders.get(uri.scheme);
 189 |       if (provider) await provider.writeFile(uri, Buffer.from(content), {});
 190 |       this.isDirty = false;
 191 |       return true;
 192 |     },
 193 |     replaceText(text) {
 194 |       if (state.providers.has(uri.scheme)) return false;
 195 |       content = text;
 196 |       this.version += 1;
 197 |       this.isDirty = true;
 198 |       for (const handler of state.changeHandlers) handler({ document, contentChanges: [{ text }] });
 199 |       return true;
 200 |     },
 201 |   };
 202 |   return document;
 203 | }
 204 |
 205 | /**
 206 |  * Open or fetch the fake document for a URI.
 207 |  *
 208 |  * Provider documents are re-read from their provider on every access so a
 209 |  * `view/render` is visible exactly as a real virtual document reflects it.
 210 |  *
 211 |  * @param {Uri} uri
 212 |  * @param {string} [content]
 213 |  * @returns {object}
 214 |  */
 215 | function openDocument(uri, content) {
 216 |   const key = uri.toString();
 217 |   if (state.fileProviders.has(uri.scheme)) {
 218 |     const existing = state.documents.get(key);
 219 |     if (existing && !existing.isClosed) return existing;
 220 |     const document = makeDocument(uri, Buffer.from(state.fileProviders.get(uri.scheme).readFile(uri)).toString());
 221 |     state.documents.set(key, document);
 222 |     return document;
 223 |   }
 224 |   if (state.providers.has(uri.scheme)) {
 225 |     const text = providerText(uri);
 226 |     const document = makeDocument(uri, text);
 227 |     Object.defineProperty(document, 'getText', { value: () => providerText(uri) });
 228 |     Object.defineProperty(document, 'lineCount', {
 229 |       get: () => providerText(uri).split('\n').length,
 230 |     });
 231 |     Object.defineProperty(document, 'lineAt', {
 232 |       value: (line) => ({ text: providerText(uri).split('\n')[line] || '' }),
 233 |     });
 234 |     state.documents.set(key, document);
 235 |     return document;
 236 |   }
 237 |   const body = content !== undefined ? content : (fs.existsSync(uri.path) ? fs.readFileSync(uri.path, 'utf8') : '');
 238 |   const document = makeDocument(uri, body);
 239 |   state.documents.set(key, document);
 240 |   return document;
 241 | }
 242 |
 243 | /**
 244 |  * The editor currently showing a document, creating one when needed.
 245 |  *
 246 |  * @param {object} document
 247 |  * @returns {object}
 248 |  */
 249 | function editorFor(document) {
 250 |   let editor = state.editors.find(
 251 |     (candidate) => candidate.document.uri.toString() === document.uri.toString()
 252 |   );
 253 |   if (!editor) {
 254 |     const position = new Position(0, 0);
 255 |     editor = {
 256 |       document,
 257 |       selection: new Selection(position, position),
 258 |       revealRange() {},
 259 |       async edit(callback) {
 260 |         let text;
 261 |         callback({ replace(range, replacement) { text = replacement; } });
 262 |         return document.replaceText(text);
 263 |       },
 264 |     };
 265 |     state.editors.push(editor);
 266 |   }
 267 |   return editor;
 268 | }
 269 |
 270 | /**
 271 |  * Create a scriptable QuickPick.
 272 |  *
 273 |  * @returns {object}
 274 |  */
 275 | function createQuickPick() {
 276 |   const quickPick = {
 277 |     title: '',
 278 |     placeholder: '',
 279 |     items: [],
 280 |     selectedItems: [],
 281 |     value: '',
 282 |     visible: false,
 283 |     acceptHandler: undefined,
 284 |     changeHandler: undefined,
 285 |     hideHandler: undefined,
 286 |     show() {
 287 |       this.visible = true;
 288 |     },
 289 |     hide() {
 290 |       this.visible = false;
 291 |       if (this.hideHandler) this.hideHandler();
 292 |     },
 293 |     dispose() {
 294 |       this.visible = false;
 295 |     },
 296 |     onDidAccept(handler) {
 297 |       this.acceptHandler = handler;
 298 |     },
 299 |     onDidChangeValue(handler) {
 300 |       this.changeHandler = handler;
 301 |     },
 302 |     onDidHide(handler) {
 303 |       this.hideHandler = handler;
 304 |     },
 305 |   };
 306 |   Object.defineProperty(quickPick, 'value', {
 307 |     get() {
 308 |       return this._value || '';
 309 |     },
 310 |     set(next) {
 311 |       this._value = next;
 312 |       if (this.changeHandler) this.changeHandler(next);
 313 |     },
 314 |   });
 315 |   /** Select one item as a user would and fire accept. @param {object} item */
 316 |   quickPick.select = function select(item) {
 317 |     this.selectedItems = [item];
 318 |     if (this.acceptHandler) this.acceptHandler();
 319 |   };
 320 |   state.quickPicks.push(quickPick);
 321 |   state.lastQuickPick = quickPick;
 322 |   return quickPick;
 323 | }
 324 |
 325 | const vscode = {
 326 |   Uri,
 327 |   EventEmitter,
 328 |   Position,
 329 |   Selection,
 330 |   Range,
 331 |   WorkspaceEdit,
 332 |   SemanticTokensLegend,
 333 |   SemanticTokens,
 334 |   QuickPickItemKind: { Separator: -1, Default: 0 },
 335 |   TextEditorRevealType: { InCenterIfOutsideViewport: 0 },
 336 |   FileType: { File: 1 },
 337 |   FileChangeType: { Changed: 1 },
 338 |   FileSystemError: {
 339 |     FileNotFound: () => new Error('Draft not found'),
 340 |     NoPermissions: (text) => new Error(text),
 341 |   },
 342 |
 343 |   workspace: {
 344 |     get workspaceFolders() {
 345 |       return state.workspaceFolder
 346 |         ? [{ uri: Uri.file(state.workspaceFolder) }]
 347 |         : undefined;
 348 |     },
 349 |     getConfiguration(section) {
 350 |       return {
 351 |         /**
 352 |          * @param {string} name
 353 |          * @param {unknown} [fallback]
 354 |          */
 355 |         get(name, fallback) {
 356 |           const scoped = state.configuration[`${section}.${name}`];
 357 |           return scoped === undefined ? fallback : scoped;
 358 |         },
 359 |       };
 360 |     },
 361 |     registerTextDocumentContentProvider(scheme, provider) {
 362 |       state.providers.set(scheme, provider);
 363 |       return { dispose() {} };
 364 |     },
 365 |     registerFileSystemProvider(scheme, provider) {
 366 |       state.fileProviders.set(scheme, provider);
 367 |       return { dispose() {} };
 368 |     },
 369 |     onDidChangeTextDocument(handler) {
 370 |       state.changeHandlers.push(handler);
 371 |       return { dispose() {} };
 372 |     },
 373 |     async applyEdit(edit) {
 374 |       for (const replacement of edit.edits) {
 375 |         const document = state.documents.get(replacement.uri.toString());
 376 |         if (!document || document.isClosed || !document.replaceText(replacement.text)) return false;
 377 |       }
 378 |       return true;
 379 |     },
 380 |     onDidCloseTextDocument(handler) {
 381 |       state.closeHandlers.push(handler);
 382 |       return { dispose() {} };
 383 |     },
 384 |     async openTextDocument(input) {
 385 |       if (input instanceof Uri) {
 386 |         return openDocument(input);
 387 |       }
 388 |       return openDocument(Uri.from({ scheme: 'untitled', path: '/untitled' }), input.content || '');
 389 |     },
 390 |   },
 391 |
 392 |   languages: {
 393 |     async setTextDocumentLanguage(document, languageId) {
 394 |       document.languageId = languageId;
 395 |       // Real VSCode closes and reopens the document for a language change.
 396 |       for (const handler of state.closeHandlers) await handler(document);
 397 |       return document;
 398 |     },
 399 |     registerDocumentSemanticTokensProvider(selector, provider, legend) {
 400 |       state.semanticProviders.push({ selector, provider, legend });
 401 |       return { dispose() {} };
 402 |     },
 403 |   },
 404 |
 405 |   commands: {
 406 |     registerCommand(name, callback) {
 407 |       state.commands.set(name, callback);
 408 |       return { dispose() {} };
 409 |     },
 410 |     async executeCommand(name, ...args) {
 411 |       state.executed.push({ name, args });
 412 |       if (name === 'setContext') {
 413 |         state.contextValues.set(args[0], args[1]);
 414 |       }
 415 |       if (name === 'workbench.action.files.revert') {
 416 |         const uri = args[0] || state.activeEditor?.document.uri;
 417 |         const document = state.documents.get(uri.toString());
 418 |         const provider = state.fileProviders.get(uri.scheme);
 419 |         if (document && provider) {
 420 |           document.replaceText(Buffer.from(provider.readFile(uri)).toString());
 421 |           document.isDirty = false;
 422 |         }
 423 |       }
 424 |       if (state.commands.has(name)) return state.commands.get(name)(...args);
 425 |     },
 426 |   },
 427 |
 428 |   window: {
 429 |     get activeTextEditor() {
 430 |       return state.activeEditor;
 431 |     },
 432 |     get visibleTextEditors() {
 433 |       return state.editors;
 434 |     },
 435 |     tabGroups: {
 436 |       get all() {
 437 |         return state.editors.map((editor) => ({
 438 |           tabs: [{ input: { uri: editor.document.uri } }],
 439 |         }));
 440 |       },
 441 |       /** Managed drafts watch tab changes to catch raw tab close teardown. */
 442 |       onDidChangeTabs: new EventEmitter().event,
 443 |       async close(tabs) {
 444 |         for (const tab of Array.isArray(tabs) ? tabs : [tabs]) {
 445 |           const document = state.documents.get(tab.input.uri.toString());
 446 |           if (document) await vscode.__test.closeDocument(document);
 447 |         }
 448 |         return true;
 449 |       },
 450 |     },
 451 |     createOutputChannel() {
 452 |       return {
 453 |         /** @param {string} line */
 454 |         appendLine(line) {
 455 |           state.output.push(line);
 456 |         },
 457 |         dispose() {},
 458 |       };
 459 |     },
 460 |     onDidChangeActiveTextEditor(handler) {
 461 |       state.activeEditorHandlers.push(handler);
 462 |       return { dispose() {} };
 463 |     },
 464 |     async showTextDocument(document) {
 465 |       const editor = editorFor(document);
 466 |       state.activeEditor = editor;
 467 |       for (const handler of state.activeEditorHandlers) handler(editor);
 468 |       return editor;
 469 |     },
 470 |     createQuickPick,
 471 |     async showQuickPick(items) {
 472 |       if (state.quickPickAnswer) {
 473 |         return state.quickPickAnswer(items);
 474 |       }
 475 |       return items && items[0];
 476 |     },
 477 |     async showInputBox() {
 478 |       return state.inputAnswers.shift();
 479 |     },
 480 |     showErrorMessage(text) {
 481 |       state.messages.push({ level: 'error', text });
 482 |     },
 483 |     showWarningMessage(text) {
 484 |       state.messages.push({ level: 'warn', text });
 485 |     },
 486 |     showInformationMessage(text) {
 487 |       state.messages.push({ level: 'info', text });
 488 |       if (state.informationAnswer) return state.informationAnswer;
 489 |       return undefined;
 490 |     },
 491 |   },
 492 |
 493 |   env: {
 494 |     clipboard: {
 495 |       /** @param {string} text */
 496 |       async writeText(text) {
 497 |         state.clipboard.push(text);
 498 |       },
 499 |     },
 500 |   },
 501 |
 502 |   /** Test-only controls; the extension never calls these. */
 503 |   __test: {
 504 |     /** Drop every recorded interaction. */
 505 |     reset() {
 506 |       state.commands.clear();
 507 |       state.providers.clear();
 508 |       state.fileProviders.clear();
 509 |       state.documents.clear();
 510 |       state.editors.length = 0;
 511 |       state.activeEditor = undefined;
 512 |       state.messages.length = 0;
 513 |       state.output.length = 0;
 514 |       state.contextValues.clear();
 515 |       state.executed.length = 0;
 516 |       state.quickPicks.length = 0;
 517 |       state.lastQuickPick = undefined;
 518 |       state.quickPickAnswer = undefined;
 519 |       state.inputAnswers.length = 0;
 520 |       state.informationAnswer = undefined;
 521 |       state.closeHandlers.length = 0;
 522 |       state.changeHandlers.length = 0;
 523 |       state.activeEditorHandlers.length = 0;
 524 |       state.workspaceFolder = undefined;
 525 |       state.configuration = {};
 526 |       state.clipboard.length = 0;
 527 |       state.semanticProviders.length = 0;
 528 |     },
 529 |     get commands() {
 530 |       return state.commands;
 531 |     },
 532 |     get documents() {
 533 |       return state.documents;
 534 |     },
 535 |     get editors() {
 536 |       return state.editors;
 537 |     },
 538 |     get messages() {
 539 |       return state.messages;
 540 |     },
 541 |     get output() {
 542 |       return state.output;
 543 |     },
 544 |     get contextValues() {
 545 |       return state.contextValues;
 546 |     },
 547 |     get quickPicks() {
 548 |       return state.quickPicks;
 549 |     },
 550 |     get lastQuickPick() {
 551 |       return state.lastQuickPick;
 552 |     },
 553 |     get clipboard() {
 554 |       return state.clipboard;
 555 |     },
 556 |     get activeEditor() {
 557 |       return state.activeEditor;
 558 |     },
 559 |     /** @param {string} folder */
 560 |     setWorkspaceFolder(folder) {
 561 |       state.workspaceFolder = folder;
 562 |     },
 563 |     /** @param {Record<string, unknown>} values dotted `section.key` settings */
 564 |     setConfiguration(values) {
 565 |       Object.assign(state.configuration, values);
 566 |     },
 567 |     /** @param {string|undefined} answer */
 568 |     queueInputAnswer(answer) {
 569 |       state.inputAnswers.push(answer);
 570 |     },
 571 |     /** @param {(items: object[]) => unknown} answer */
 572 |     setQuickPickAnswer(answer) {
 573 |       state.quickPickAnswer = answer;
 574 |     },
 575 |     /** @param {unknown} answer */
 576 |     setInformationAnswer(answer) {
 577 |       state.informationAnswer = answer;
 578 |     },
 579 |     /** Simulate the user closing a document. @param {object} document */
 580 |     async closeDocument(document) {
 581 |       document.isClosed = true;
 582 |       state.editors = state.editors.filter((editor) => editor.document !== document);
 583 |       for (const handler of state.closeHandlers) await handler(document);
 584 |     },
 585 |   },
 586 | };
 587 |
 588 | module.exports = vscode;
```
### Unit/integration names and important actual seams


clients/vscode/test/test.js declarations/checks (all matches, source line):
`clients/vscode/test/test.js:24: function test(name, body) {`
`clients/vscode/test/test.js:35: test('a status descriptor derives the status scope', () => {`
`clients/vscode/test/test.js:48: test('a menu descriptor uses the shared menu scope and keeps capture', () => {`
`clients/vscode/test/test.js:58: test('text mode turns capture off', () => {`
`clients/vscode/test/test.js:68: test('no descriptor means inactive', () => {`
`clients/vscode/test/test.js:81: test(`render contract: ${vector.name}`, () => {`
`clients/vscode/test/test.js:140: test('QuickPick rows follow the menu groups', () => {`
`clients/vscode/test/test.js:168: test('typed characters become canonical key steps', () => {`
`clients/vscode/test/test.js:178: test('QuickPick rows preserve server activation steps for every canonical shape', () => {`
`clients/vscode/test/test.js:199: test('generated and mirrored bindings require editor focus and exclude widgets', () => {`
`clients/vscode/test/test.js:222: test('canonical status Enter, fold and refresh bindings keep the focus gate', () => {`
`clients/vscode/test/test.js:244: test('semantic-token types map to theme scopes', () => {`
`clients/vscode/test/test.js:290: test('one generic dispatcher carries every contributed action binding', () => {`
`clients/vscode/test/test.js:317: test('managed draft effects accept only the exact full-sync announcement', () => {`
`clients/vscode/test/test.js:346: test('JSONC comments and trailing commas are stripped', () => {`
`clients/vscode/test/test.js:352: test('JSONC parser preserves comment-like escaped strings and rejects malformed input', () => {`
`clients/vscode/test/test.js:358: test('native fold shortcuts map to magit.invoke entries', () => {`
`clients/vscode/test/test.js:381: test('fold levels become detail changes', () => {`
`clients/vscode/test/test.js:391: test('shared prompt vectors preserve protocol answer semantics', () => {`
`clients/vscode/test/test.js:420: test('executable VSCode e2e paths enforce no-vision policy', noVisionPolicy);`

clients/vscode/test/integration.js declarations/checks (all matches, source line):
`clients/vscode/test/integration.js:131: function delay(milliseconds) {`
`clients/vscode/test/integration.js:143: async function waitFor(condition, label, timeout = 10000) {`
`clients/vscode/test/integration.js:159: function git(repo, args) {`
`clients/vscode/test/integration.js:168: function createRepository() {`
`clients/vscode/test/integration.js:185: function loadExtension() {`
`clients/vscode/test/integration.js:187: Module._load = function load(request, parent, isMain) {`
`clients/vscode/test/integration.js:200: async function run() {`

clients/vscode/test/open.test.js declarations/checks (all matches, source line):
`clients/vscode/test/open.test.js:7: module.exports = async function testStatusOpenLifecycle() {`
`clients/vscode/test/open.test.js:155: Module._load = function load(request, parent, isMain) {`

clients/vscode/test/selection-integration.js declarations/checks (all matches, source line):
`clients/vscode/test/selection-integration.js:12: function select(vscode, editor, anchor, active) {`
`clients/vscode/test/selection-integration.js:23: function checkRequest(records, before, method, active, rows) {`
`clients/vscode/test/selection-integration.js:40: async function checkShapes({ vscode, commands, client, editor }) {`
`clients/vscode/test/selection-integration.js:48: client.sendRequest = function sendRequest(method, params) {`
`clients/vscode/test/selection-integration.js:103: function git(repo, ...args) {`
`clients/vscode/test/selection-integration.js:108: async function checkGitSelections({ vscode, commands, client }) {`
`clients/vscode/test/selection-integration.js:249: module.exports = { checkShapes, checkGitSelections };`

`test.js` test registrations and core logic: full file is 436 lines; function/name index above is exhaustive for declarations/tests. Parameterized test implementation and cases are here:

### `clients/vscode/test/test.js` lines 1-100 of 436
```text
   1 | 'use strict';
   2 |
   3 | const assert = require('node:assert/strict');
   4 |
   5 | const context = require('../lib/context');
   6 | const render = require('../lib/render');
   7 | const menu = require('../lib/menu');
   8 | const foldkeys = require('../lib/foldkeys');
   9 | const prompt = require('../lib/prompt');
  10 | const edit = require('../lib/edit');
  11 | const promptVectors = require('../../../tests/data/client-contracts.json').prompts;
  12 | const noVisionPolicy = require('./no-vision');
  13 | const renderVectors = require('../../../tests/data/client-contracts.json').renders;
  14 |
  15 | const failures = [];
  16 | let passed = 0;
  17 |
  18 | /**
  19 |  * Run one named test and record its failure instead of stopping the suite.
  20 |  *
  21 |  * @param {string} name
  22 |  * @param {() => void} body
  23 |  */
  24 | function test(name, body) {
  25 |   try {
  26 |     body();
  27 |     passed += 1;
  28 |   } catch (error) {
  29 |     failures.push({ name, error });
  30 |   }
  31 | }
  32 |
  33 | // lib/context.js -----------------------------------------------------------
  34 |
  35 | test('a status descriptor derives the status scope', () => {
  36 |   const keys = context.deriveContext(
  37 |     { id: 'v1', scope: 'status', capture: false },
  38 |     'interactive'
  39 |   );
  40 |   assert.deepEqual(keys, {
  41 |     'magit.active': true,
  42 |     'magit.scope': 'status',
  43 |     'magit.textMode': false,
  44 |     'magit.capture': false,
  45 |   });
  46 | });
  47 |
  48 | test('a menu descriptor uses the shared menu scope and keeps capture', () => {
  49 |   const keys = context.deriveContext(
  50 |     { id: 'v2', scope: 'menu.commit', capture: true },
  51 |     'interactive'
  52 |   );
  53 |   assert.equal(keys['magit.scope'], 'menu');
  54 |   assert.equal(keys['magit.capture'], true);
  55 |   assert.equal(keys['magit.textMode'], false);
  56 | });
  57 |
  58 | test('text mode turns capture off', () => {
  59 |   const keys = context.deriveContext(
  60 |     { id: 'v3', scope: 'menu.rebase', capture: true },
  61 |     'text'
  62 |   );
  63 |   assert.equal(keys['magit.scope'], 'menu');
  64 |   assert.equal(keys['magit.textMode'], true);
  65 |   assert.equal(keys['magit.capture'], false);
  66 | });
  67 |
  68 | test('no descriptor means inactive', () => {
  69 |   assert.deepEqual(context.deriveContext(undefined, 'interactive'), {
  70 |     'magit.active': false,
  71 |     'magit.scope': '',
  72 |     'magit.textMode': false,
  73 |     'magit.capture': false,
  74 |   });
  75 |   assert.equal(context.inactiveContext()['magit.active'], false);
  76 | });
  77 |
  78 | // lib/render.js ------------------------------------------------------------
  79 |
  80 | for (const vector of renderVectors) {
  81 |   test(`render contract: ${vector.name}`, () => {
  82 |     const result = render.applyRender(vector.initialLines, vector.initialVersion, vector.render);
  83 |     assert.deepEqual(result.lines, vector.expectedLines);
  84 |     assert.equal(result.version, vector.expectedVersion);
  85 |     assert.equal(result.status, vector.outcome === 'resync' ? 'resync' : vector.render.text ? 'full' : vector.render.edits ? 'edits' : 'none');
  86 |   });
  87 | }
  88 |
  89 | // lib/menu.js --------------------------------------------------------------
  90 |
  91 | const MENU_MODEL = {
  92 |   id: 'commit',
  93 |   title: 'Commit',
  94 |   groups: [
  95 |     {
  96 |       title: 'Arguments',
  97 |       items: [
  98 |         {
  99 |           kind: 'switch',
 100 |           key: '-a',
```
### `clients/vscode/test/test.js` lines 135-245 of 436
```text
 135 |     },
 136 |   ],
 137 |   savedArgs: { switches: [], options: {} },
 138 | };
 139 |
 140 | test('QuickPick rows follow the menu groups', () => {
 141 |   const rows = menu.quickPickItems(MENU_MODEL);
 142 |   assert.equal(rows.length, 6);
 143 |
 144 |   assert.equal(rows[0].kind, 'separator');
 145 |   assert.equal(rows[0].label, 'Arguments');
 146 |
 147 |   assert.equal(rows[1].kind, 'item');
 148 |   assert.equal(rows[1].key, '-a');
 149 |   assert.equal(rows[1].label, '-a  Stage all modified and deleted files');
 150 |   assert.match(rows[1].description, /--all/);
 151 |   assert.match(rows[1].description, /on/);
 152 |   assert.equal(rows[1].on, true);
 153 |
 154 |   assert.equal(rows[2].key, '=A');
 155 |   assert.equal(rows[2].value, 'Alice');
 156 |   assert.match(rows[2].description, /--author=/);
 157 |
 158 |   assert.equal(rows[3].label, 'Create');
 159 |   assert.equal(rows[4].key, 'c');
 160 |   assert.equal(rows[4].status, 'implemented');
 161 |
 162 |   assert.equal(rows[5].key, 'e');
 163 |   assert.equal(rows[5].status, 'planned');
 164 |   assert.equal(rows[5].roadmapId, 'R-4.1');
 165 |   assert.match(rows[5].description, /planned R-4\.1/);
 166 | });
 167 |
 168 | test('typed characters become canonical key steps', () => {
 169 |   assert.deepEqual(menu.routeTyped('c'), ['c']);
 170 |   assert.deepEqual(menu.routeTyped('-a'), ['-', 'a']);
 171 |   assert.deepEqual(menu.routeTyped('=A'), ['=', 'A']);
 172 |   assert.deepEqual(menu.routeTyped('?'), ['?']);
 173 |   assert.deepEqual(menu.routeTyped(' '), ['Space']);
 174 |   assert.deepEqual(menu.routeTyped('+'), ['Plus']);
 175 |   assert.deepEqual(menu.routeTyped(undefined), []);
 176 | });
 177 |
 178 | test('QuickPick rows preserve server activation steps for every canonical shape', () => {
 179 |   const cases = [
 180 |     ['-a', ['-', 'a']], ['=A', ['=', 'A']], ['g l', ['g', 'l']],
 181 |     ['ctrl+t', ['ctrl+t']], ['Enter', ['Enter']], ['G', ['G']], ['Plus', ['Plus']],
 182 |   ];
 183 |   for (const [key, activationSteps] of cases) {
 184 |     const item = { kind: 'action', key, activationSteps, label: 'Planned',
 185 |       status: { status: 'planned', roadmapId: 'R-4.1' } };
 186 |     const row = menu.quickPickItems({ groups: [{ title: 'Actions', items: [item] }] })[1];
 187 |     assert.equal(row.key, key);
 188 |     assert.equal(row.activationSteps, activationSteps, 'no client parsing or conversion');
 189 |     assert.equal(row.status, 'planned');
 190 |     assert.equal(row.roadmapId, 'R-4.1');
 191 |   }
 192 |   assert.deepEqual(menu.routeTyped('ctrl+t'), ['c', 't', 'r', 'l', 'Plus', 't']);
 193 |   assert.deepEqual(menu.routeTyped('g l'), ['g', 'Space', 'l']);
 194 |   assert.deepEqual(menu.routeTyped('Enter'), ['E', 'n', 't', 'e', 'r']);
 195 | });
 196 |
 197 | // lib/foldkeys.js ----------------------------------------------------------
 198 |
 199 | test('generated and mirrored bindings require editor focus and exclude widgets', () => {
 200 |   const generated = require('../package.json').contributes.keybindings;
 201 |   const mirrored = foldkeys.mapFoldKeys([{ key: 'ctrl+k ctrl+l', command: 'editor.toggleFold' }]);
 202 |   for (const entry of [...generated, ...mirrored]) {
 203 |     const terms = entry.when.split(' && ');
 204 |     for (const term of ['magit.active', 'editorTextFocus', '!quickInputVisible',
 205 |       '!suggestWidgetVisible', '!findWidgetVisible', '!renameInputVisible']) {
 206 |       assert(terms.includes(term), `${entry.key}: missing ${term}`);
 207 |     }
 208 |     // Test the focus gate independently of scope/text-mode variants.
 209 |     const gate = (state) => terms.every((term) => {
 210 |       if (term.startsWith('magit.scope') || term.includes('magit.textMode')) return true;
 211 |       return term.startsWith('!') ? !state[term.slice(1)] : !!state[term];
 212 |     });
 213 |     const status = { 'magit.active': true, editorTextFocus: true };
 214 |     assert(gate(status));
 215 |     assert(!gate({ ...status, editorTextFocus: false }), 'sidebar/terminal/input focus');
 216 |     for (const widget of ['quickInputVisible', 'suggestWidgetVisible', 'findWidgetVisible', 'renameInputVisible']) {
 217 |       assert(!gate({ ...status, [widget]: true }), widget);
 218 |     }
 219 |   }
 220 | });
 221 |
 222 | test('canonical status Enter, fold and refresh bindings keep the focus gate', () => {
 223 |   const generated = require('../package.json').contributes.keybindings;
 224 |   const required = [
 225 |     ['enter', 'common.visit'],
 226 |     ['tab', 'common.toggle-section'],
 227 |     ['g', 'common.refresh'],
 228 |     ['f5', 'common.refresh'],
 229 |   ];
 230 |   for (const [key, command] of required) {
 231 |     const matches = generated.filter((entry) => entry.key === key
 232 |       && entry.args && entry.args.command === command
 233 |       && entry.when.includes("magit.scope == 'status'"));
 234 |     assert.equal(matches.length, 1, `${key} -> ${command} for status`);
 235 |     const terms = matches[0].when.split(' && ');
 236 |     for (const term of ['magit.active', 'editorTextFocus', '!quickInputVisible',
 237 |       '!suggestWidgetVisible', '!findWidgetVisible', '!renameInputVisible',
 238 |       '!magit.textMode']) {
 239 |       assert(terms.includes(term), `${key}: missing ${term}`);
 240 |     }
 241 |   }
 242 | });
 243 |
 244 | test('semantic-token types map to theme scopes', () => {
 245 |   const contributions = require('../package.json').contributes;
```
### `clients/vscode/test/test.js` lines 285-435 of 436
```text
 285 |     true,
 286 |     'semantic highlighting is not enabled for the magit language'
 287 |   );
 288 | });
 289 |
 290 | test('one generic dispatcher carries every contributed action binding', () => {
 291 |   const contributions = require('../package.json').contributes;
 292 |   const clientCommands = contributions.commands.map((entry) => entry.command).sort();
 293 |   assert.deepEqual(clientCommands, [
 294 |     'magit.cancelEdit',
 295 |     'magit.closeEdit',
 296 |     'magit.confirmEdit',
 297 |     'magit.invoke',
 298 |     'magit.key',
 299 |     'magit.mirrorFoldShortcuts',
 300 |     'magit.open',
 301 |   ]);
 302 |   // No contributed VSCode command may be named after a server action; every
 303 |   // action binding goes through `magit.invoke` with a canonical server ID.
 304 |   for (const perAction of ['magit.refresh', 'magit.toggle-fold', 'magit.show-section',
 305 |     'magit.hide-section', 'magit.stage']) {
 306 |     assert(!clientCommands.includes(perAction), perAction);
 307 |   }
 308 |   assert.equal(contributions.keybindings.length > 100, true);
 309 |   for (const binding of contributions.keybindings) {
 310 |     assert.equal(binding.command, 'magit.invoke', binding.key);
 311 |     assert.equal(typeof binding.args?.command, 'string', binding.key);
 312 |     assert.match(binding.args.command, /^[a-z][a-z0-9-]*\.[a-z][a-z0-9-]*$/);
 313 |     assert(!clientCommands.includes(binding.args.command), binding.key);
 314 |   }
 315 | });
 316 |
 317 | test('managed draft effects accept only the exact full-sync announcement', () => {
 318 |   const effect = { type: 'editDocument', mode: 'realFile', runId: 'run-1',
 319 |     uri: 'file:///repo/.git/COMMIT_EDITMSG', version: 1, text: 'exact\r\n', kind: 'commit-message' };
 320 |   assert(edit.isEditEffect(effect));
 321 |   for (const invalid of [{ ...effect, path: '/tmp/message' }, { ...effect, mode: 'unsupported' },
 322 |     { ...effect, uri: 'magit-commit://other/message' }, { ...effect, version: 2 },
 323 |     { ...effect, requestId: 4 }]) assert(!edit.isEditEffect(invalid));
 324 |   assert.equal(edit.editingContext(effect)['magit.active'], false, 'native typing is not a view keymap');
 325 |   assert.equal(edit.draftAction('common.quit'), 'close');
 326 |   assert.equal(edit.draftAction('commit-message.history-prev'), undefined);
 327 |   assert.equal(edit.draftAction('commit-message.history-next'), undefined);
 328 | });
 329 |
 330 | const KEYBINDINGS_JSONC = `[
 331 |   // Fold the region under the cursor.
 332 |   { "key": "ctrl+shift+[", "command": "editor.fold" },
 333 |   /* Unfold it again. */
 334 |   { "key": "ctrl+shift+]", "command": "editor.unfold", "when": "editorTextFocus" },
 335 |   { "key": "ctrl+k ctrl+l", "command": "editor.toggleFold" },
 336 |   { "key": "ctrl+k ctrl+0", "command": "editor.foldAll" },
 337 |   { "key": "ctrl+shift+p", "command": "workbench.action.showCommands" },
 338 |   {
 339 |     "key": "ctrl+shift+[",
 340 |     "command": "editor.foldRecursively",
 341 |     "mac": "cmd+alt+[",
 342 |     "args": { "note": "https://example.com // not a comment" }
 343 |   },
 344 | ]`;
 345 |
 346 | test('JSONC comments and trailing commas are stripped', () => {
 347 |   const parsed = foldkeys.parseJsonc(KEYBINDINGS_JSONC);
 348 |   assert.equal(parsed.length, 6);
 349 |   assert.equal(parsed[5].command, 'editor.foldRecursively');
 350 | });
 351 |
 352 | test('JSONC parser preserves comment-like escaped strings and rejects malformed input', () => {
 353 |   const parsed = foldkeys.parseJsonc('[{"key":"x\\\"//text","command":"editor.fold","note":"/* text */",},]');
 354 |   assert.equal(parsed[0].key, 'x"//text');
 355 |   assert.throws(() => foldkeys.parseJsonc('[{"command":]'), /Invalid JSONC.*line/);
 356 | });
 357 |
 358 | test('native fold shortcuts map to magit.invoke entries', () => {
 359 |   const entries = foldkeys.mapFoldKeys(KEYBINDINGS_JSONC);
 360 |   assert.equal(entries.length, 5);
 361 |
 362 |   assert.deepEqual(entries[0], {
 363 |     key: 'ctrl+shift+[',
 364 |     command: 'magit.invoke',
 365 |     args: { command: 'common.hide-section' },
 366 |     when: 'magit.active && !magit.textMode && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible',
 367 |   });
 368 |   assert.equal(entries[1].args.command, 'common.show-section');
 369 |   assert.equal(entries[2].args.command, 'common.toggle-section');
 370 |   assert.equal(entries[3].args.command, 'common.hide-all');
 371 |
 372 |   // The workbench command is not a fold shortcut: order skips it.
 373 |   assert.equal(entries[4].key, 'ctrl+shift+[');
 374 |   assert.deepEqual(entries[4].args, {
 375 |     command: 'common.hide-section',
 376 |     args: { recursive: true },
 377 |   });
 378 |   assert.equal(entries[4].mac, 'cmd+alt+[');
 379 | });
 380 |
 381 | test('fold levels become detail changes', () => {
 382 |   const entries = foldkeys.mapFoldKeys([
 383 |     { key: 'ctrl+k ctrl+1', command: 'editor.foldLevel1' },
 384 |   ]);
 385 |   assert.equal(entries.length, 1);
 386 |   assert.equal(entries[0].args.command, 'common.less-detail');
 387 | });
 388 |
 389 | // lib/prompt.js ------------------------------------------------------------
 390 |
 391 | test('shared prompt vectors preserve protocol answer semantics', () => {
 392 |   for (const vector of promptVectors) {
 393 |     const params = vector.params;
 394 |     let answer;
 395 |     if (params.kind === 'confirm') {
 396 |       const items = prompt.promptItems(params);
 397 |       if (vector.focus) {
 398 |         assert.deepEqual(items.map((item) => item.label), vector.focus, vector.name);
 399 |       }
 400 |       answer = prompt.answerFor(params, vector.response === null
 401 |         ? undefined : { id: vector.response });
 402 |     } else if (params.kind === 'text') {
 403 |       answer = prompt.answerFor(params, vector.response);
 404 |     } else if (params.kind === 'choice') {
 405 |       answer = vector.response === '__custom__'
 406 |         ? prompt.customChoice(vector.customResponse)
 407 |         : prompt.answerFor(params, vector.response === null
 408 |           ? undefined : { id: vector.response });
 409 |     } else {
 410 |       answer = vector.response === null
 411 |         ? { cancelled: true }
 412 |         : prompt.answerFor(params, vector.response.map((id) => ({ id })));
 413 |     }
 414 |     assert.deepEqual(answer, vector.answer, vector.name);
 415 |   }
 416 | });
 417 |
 418 | // summary ------------------------------------------------------------------
 419 |
 420 | test('executable VSCode e2e paths enforce no-vision policy', noVisionPolicy);
 421 |
 422 | if (failures.length > 0) {
 423 |   for (const failure of failures) {
 424 |     console.error(`FAIL: ${failure.name}`);
 425 |     console.error(failure.error && failure.error.stack ? failure.error.stack : failure.error);
 426 |   }
 427 |   console.error(`\n${failures.length} failed, ${passed} passed`);
 428 |   process.exit(1);
 429 | }
 430 |
 431 | require('./open.test')().then(() => {
 432 |   console.log(`ok ${passed} tests`);
 433 | }).catch((error) => {
 434 |   console.error(error && error.stack ? error.stack : error);
 435 |   process.exitCode = 1;
```

`integration.js` actual named run and status-open/refresh/idle/document-text/env seams:

### `clients/vscode/test/integration.js` lines 1-40 of 653
```text
   1 | 'use strict';
   2 |
   3 | /**
   4 |  * Node integration test: activate the real extension against the real server.
   5 |  *
   6 |  * A scripted `vscode` module supplies the editor widgets and virtual
   7 |  * documents; the fake `LanguageClient` speaks LSP to `target/debug/magit-lsp`
   8 |  * over stdio. The test proves the thin-client contract end to end: open a view,
   9 |  * render the server's text verbatim, press a key that triggers a render, use a
  10 |  * menu QuickPick, and answer a server prompt.
  11 |  */
  12 |
  13 | const assert = require('node:assert/strict');
  14 | const { execFileSync } = require('node:child_process');
  15 | const fs = require('node:fs');
  16 | const os = require('node:os');
  17 | const path = require('node:path');
  18 | const Module = require('node:module');
  19 |
  20 | const vscode = require('./fake/vscode');
  21 | const { LanguageClient, instances } = require('./fake/languageclient');
  22 |
  23 | /** Scripted client with explicit, validated work-done progress support. */
  24 | class IntegrationClient extends LanguageClient {
  25 |   constructor(id, name, serverOptions, clientOptions) {
  26 |     const initialize = clientOptions.middleware.initialize;
  27 |     super(id, name, serverOptions, {
  28 |       ...clientOptions,
  29 |       middleware: {
  30 |         ...clientOptions.middleware,
  31 |         initialize(params, next, token) {
  32 |           params.capabilities.window = { workDoneProgress: true };
  33 |           return initialize(params, next, token);
  34 |         },
  35 |       },
  36 |     });
  37 |     this.createdProgress = new Set();
  38 |     this.progress = new Map();
  39 |     this.onNotification('$/progress', ({ token, value }) => {
  40 |       assert(this.createdProgress.has(token), 'progress must be created before use');
```
### `clients/vscode/test/integration.js` lines 131-215 of 653
```text
 131 | function delay(milliseconds) {
 132 |   return new Promise((resolve) => setTimeout(resolve, milliseconds));
 133 | }
 134 |
 135 | /**
 136 |  * Poll `condition` until it is truthy or the timeout expires.
 137 |  *
 138 |  * @param {() => unknown} condition
 139 |  * @param {string} label
 140 |  * @param {number} [timeout]
 141 |  * @returns {Promise<void>}
 142 |  */
 143 | async function waitFor(condition, label, timeout = 10000) {
 144 |   const deadline = Date.now() + timeout;
 145 |   while (!condition()) {
 146 |     if (Date.now() > deadline) {
 147 |       throw new Error(`timed out waiting for ${label}`);
 148 |     }
 149 |     await delay(20);
 150 |   }
 151 | }
 152 |
 153 | /**
 154 |  * Run Git in the fixture repository.
 155 |  *
 156 |  * @param {string} repo
 157 |  * @param {string[]} args
 158 |  */
 159 | function git(repo, args) {
 160 |   execFileSync('git', ['-C', repo, ...args], { stdio: 'ignore' });
 161 | }
 162 |
 163 | /**
 164 |  * Create a repository with one modified tracked file.
 165 |  *
 166 |  * @returns {string} repository path
 167 |  */
 168 | function createRepository() {
 169 |   const repo = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-lsp-vscode-'));
 170 |   git(repo, ['init', '-q']);
 171 |   git(repo, ['config', 'user.name', 'Test']);
 172 |   git(repo, ['config', 'user.email', 'test@example.com']);
 173 |   fs.writeFileSync(path.join(repo, 'tracked.txt'), 'original\n');
 174 |   git(repo, ['add', '.']);
 175 |   git(repo, ['commit', '-qm', 'initial']);
 176 |   fs.writeFileSync(path.join(repo, 'tracked.txt'), 'changed\n');
 177 |   return repo;
 178 | }
 179 |
 180 | /**
 181 |  * Load `extension.js` with the fake editor modules installed.
 182 |  *
 183 |  * @returns {object} the extension module
 184 |  */
 185 | function loadExtension() {
 186 |   const originalLoad = Module._load;
 187 |   Module._load = function load(request, parent, isMain) {
 188 |     if (request === 'vscode') return vscode;
 189 |     if (request === 'vscode-languageclient/node') return { LanguageClient: IntegrationClient };
 190 |     return originalLoad.call(this, request, parent, isMain);
 191 |   };
 192 |   try {
 193 |     return require('../extension');
 194 |   } finally {
 195 |     Module._load = originalLoad;
 196 |   }
 197 | }
 198 |
 199 | /** Run the scenario, reporting product output when an assertion fails. */
 200 | async function run() {
 201 |   const binary = process.env.MAGIT_LSP_SERVER_PATH
 202 |     || path.resolve(here, '../../../target/debug/magit-lsp');
 203 |   if (!fs.existsSync(binary)) {
 204 |     throw new Error(`missing ${binary}; run \`cargo build\` or \`make build\` first`);
 205 |   }
 206 |
 207 |   const repo = createRepository();
 208 |   vscode.__test.reset();
 209 |   vscode.workspace.registerFileSystemProvider('file', {
 210 |     readFile: (uri) => fs.readFileSync(uri.fsPath),
 211 |     writeFile: (uri, bytes) => fs.writeFileSync(uri.fsPath, bytes),
 212 |   });
 213 |   vscode.__test.setWorkspaceFolder(repo);
 214 |   vscode.__test.setConfiguration({
 215 |     'magit-lsp.serverPath': binary,
```
### `clients/vscode/test/integration.js` lines 228-320 of 653
```text
 228 |     // 1. Open the status view and display the server's text verbatim.
 229 |     await commands.get('magit.open')();
 230 |     await waitFor(
 231 |       () => vscode.__test.activeEditor
 232 |         && /Unstaged changes/.test(vscode.__test.activeEditor.document.getText()),
 233 |       'status view text'
 234 |     );
 235 |     const statusEditor = vscode.__test.activeEditor;
 236 |     const openedText = statusEditor.document.getText();
 237 |     assert.match(openedText, /Unstaged changes/);
 238 |     assert.equal(statusEditor.document.uri.scheme, 'magit');
 239 |     assert(
 240 |       client.requests.some((request) => request.method === 'view/open'),
 241 |       'opened the view through view/open'
 242 |     );
 243 |     assert(
 244 |       client.notes.some((note) => note.method === 'view/rendered'),
 245 |       'acknowledged the first render'
 246 |     );
 247 |
 248 |     // 2. A key press goes to the server and its render replaces the text.
 249 |     fs.writeFileSync(path.join(repo, 'added.txt'), 'new\n');
 250 |     const rendersBefore = client.notes.filter((note) => note.method === 'view/rendered').length;
 251 |     await commands.get('magit.key')({ key: 'g' });
 252 |     await waitFor(
 253 |       () => /added\.txt/.test(statusEditor.document.getText()),
 254 |       'refreshed status text'
 255 |     );
 256 |     assert(
 257 |       client.requests.some((request) => request.method === 'view/key'
 258 |         && request.params.key === 'g'
 259 |         && request.params.viewId === 'v1'),
 260 |       'forwarded the canonical key with the server view id'
 261 |     );
 262 |     assert(
 263 |       client.notes.filter((note) => note.method === 'view/rendered').length > rendersBefore,
 264 |       'acknowledged the render caused by the key press'
 265 |     );
 266 |
 267 |     await require('./selection-integration').checkShapes({ vscode, commands, client, editor: statusEditor });
 268 |
 269 |     // Static mode is a metadata-only transition: it must not invalidate the
 270 |     // virtual document or disturb the user's selection.
 271 |     const statusState = require('../lib/store').store.views.get('v1');
 272 |     const selection = new vscode.Selection(new vscode.Position(0, 0), new vscode.Position(0, 3));
 273 |     statusEditor.selection = selection;
 274 |     const linesBeforeStatic = statusState.lines;
 275 |     const versionBeforeStatic = statusState.version;
 276 |     let documentsChangedCount = 0;
 277 |     const documentsChangedSubscription = require('../lib/store').store.documentsChanged.event(() => {
 278 |       documentsChangedCount += 1;
 279 |     });
 280 |     const changedBeforeStatic = client.notes.filter((note) => note.method === 'view/rendered').length;
 281 |     await commands.get('magit.invoke')({ command: 'common.toggle-text-mode' });
 282 |     await waitFor(() => vscode.__test.contextValues.get('magit.textMode') === true,
 283 |       'text-mode context');
 284 |     assert.equal(vscode.__test.contextValues.get('magit.textMode'), true);
 285 |     assert.equal(statusEditor.selection, selection);
 286 |     assert.equal(statusState.lines, linesBeforeStatic);
 287 |     assert.equal(statusState.version, versionBeforeStatic);
 288 |     assert.equal(documentsChangedCount, 0);
 289 |     assert.equal(client.notes.filter((note) => note.method === 'view/rendered').length, changedBeforeStatic + 1);
 290 |     assert.equal(client.notes.at(-1).method, 'view/rendered');
 291 |     assert.equal(client.notes.at(-1).params.version, versionBeforeStatic);
 292 |
 293 |     // Refreshes queued while static are applied as a full server render on
 294 |     // return to interactive mode.
 295 |     fs.writeFileSync(path.join(repo, 'queued.txt'), 'queued\n');
 296 |     await commands.get('magit.key')({ key: 'g' });
 297 |     assert.equal(statusState.lines, linesBeforeStatic);
 298 |     assert.equal(statusEditor.selection, selection);
 299 |     await commands.get('magit.invoke')({ command: 'common.toggle-text-mode' });
 300 |     await waitFor(() => vscode.__test.contextValues.get('magit.textMode') === false,
 301 |       'interactive-mode context');
 302 |     await waitFor(() => /queued\.txt/.test(statusEditor.document.getText()), 'queued refresh after static mode');
 303 |     assert.equal(vscode.__test.contextValues.get('magit.textMode'), false);
 304 |     assert.notEqual(statusState.lines, linesBeforeStatic);
 305 |     documentsChangedSubscription.dispose();
 306 |
 307 |     // 3. A server menu becomes a QuickPick; selecting a row sends its key.
 308 |     let promptItems;
 309 |     vscode.__test.setQuickPickAnswer((items) => {
 310 |       promptItems = items;
 311 |       return items.find((item) => item.id === 'no');
 312 |     });
 313 |     await commands.get('magit.invoke')({ command: 'open.commit' });
 314 |     await waitFor(
 315 |       () => vscode.__test.lastQuickPick
 316 |         && vscode.__test.lastQuickPick.items.some((item) => item.key === 'c'),
 317 |       'commit menu QuickPick'
 318 |     );
 319 |     const menu = vscode.__test.lastQuickPick;
 320 |     const initialMenuState = require('../lib/store').store.views.get(
```
### `clients/vscode/test/integration.js` lines 425-445 of 653
```text
 425 |     const { store: draftStore } = require('../lib/store');
 426 |     const progress = client.progress;
 427 |     assert.equal(client.requests.find((request) => request.method === 'initialize')
 428 |       .params.capabilities.window.workDoneProgress, true);
 429 |     const draftProgress = new WeakMap();
 430 |     const head = () => execFileSync('git', ['-C', repo, 'rev-parse', 'HEAD']).toString();
 431 |     const subject = () => execFileSync('git', ['-C', repo, 'log', '-1', '--format=%s']).toString().trim();
 432 |     const idle = async (draft) => {
 433 |       const token = draftProgress.get(draft);
 434 |       assert.notEqual(token, undefined, 'draft must have observed progress begin');
 435 |       await draft.queue;
 436 |       await waitFor(() => progress.get(token) === 'end', 'commit progress end');
 437 |     };
 438 |     let probe = 0;
 439 |     let previousDraft;
 440 |     const startDraft = async () => {
 441 |       if (previousDraft) await idle(previousDraft);
 442 |       await vscode.window.showTextDocument(statusEditor.document);
 443 |       fs.writeFileSync(path.join(repo, 'tracked.txt'), `managed probe ${++probe}\n`);
 444 |       git(repo, ['add', 'tracked.txt']);
 445 |       const before = new Set(draftStore.drafts.keys());
```
### `clients/vscode/test/integration.js` lines 580-653 of 653
```text
 580 |       await idle(draft);
 581 |       assert.equal(head(), priorHead, action);
 582 |       assert.deepEqual(unchangedIndex(), priorIndex, action);
 583 |       if (draftStore.drafts.has(draft.runId)) await commands.get('magit.cancelEdit')();
 584 |     }
 585 |     const synced = client.notes.filter((note) => note.method === 'textDocument/didChange');
 586 |     assert(synced.every((note) => note.params.contentChanges.length === 1
 587 |       && Object.keys(note.params.contentChanges[0]).join() === 'text'));
 588 |
 589 |     // The displayed text is always the server's last render; never recomposed.
 590 |     assert.match(statusEditor.document.getText(), /Unstaged changes/);
 591 |     assert.equal(vscode.__test.output.filter((line) => line.startsWith('unknown effect')).length, 0);
 592 |
 593 |     // Quit is dispatched through the registered command and scoped to the
 594 |     // repository that owns the originating view. This harness uses mocked
 595 |     // VSCode APIs; it is not evidence of native tab behavior.
 596 |     const { store } = require('../lib/store');
 597 |     const secondRepo = createRepository();
 598 |     vscode.__test.setWorkspaceFolder(secondRepo);
 599 |     await commands.get('magit.open')();
 600 |     await waitFor(() => store.views.size === 2, 'second repository view');
 601 |     await commands.get('magit.invoke')({ command: 'common.quit' });
 602 |     assert.equal(store.views.size, 1, 'quit removes only the originating repository view');
 603 |     assert.equal(Array.from(store.views.values())[0].repo, repo);
 604 |     vscode.__test.setWorkspaceFolder(repo);
 605 |     await commands.get('magit.open')();
 606 |     await commands.get('magit.invoke')({ command: 'common.quit' });
 607 |     assert.equal(store.views.size, 0, 'root quit removes the affected stored view');
 608 |     await commands.get('magit.open')();
 609 |     await waitFor(() => store.views.size === 1, 'reopened repository view');
 610 |     fs.rmSync(secondRepo, { recursive: true, force: true });
 611 |     await require('./selection-integration').checkGitSelections({ vscode, commands, client });
 612 |     // A disconnected owner cannot finish or restart against a new connection;
 613 |     // server shutdown aborts the pending helper, while native text is retained.
 614 |     vscode.__test.setWorkspaceFolder(repo);
 615 |     await commands.get('magit.open')();
 616 |     fs.writeFileSync(path.join(repo, 'tracked.txt'), 'disconnect draft probe\n');
 617 |     git(repo, ['add', 'tracked.txt']);
 618 |     await commands.get('magit.invoke')({ command: 'commit.create' });
 619 |     await waitFor(() => [...draftStore.drafts.values()].some((record) => record.attached), 'disconnect draft');
 620 |     const disconnected = [...draftStore.drafts.values()].find((record) => record.attached);
 621 |     disconnected.document.replaceText('retain disconnected text\n');
 622 |     const disconnectHead = head();
 623 |     await extension.deactivate();
 624 |     assert.equal(head(), disconnectHead, 'disconnect cancels the pending commit');
 625 |     assert.equal(disconnected.document.getText(), 'retain disconnected text\n');
 626 |     assert(disconnected.disconnected);
 627 |   } finally {
 628 |     await extension.deactivate();
 629 |   }
 630 |
 631 |   const exited = await Promise.race([
 632 |     instances.at(-1).exited.then(() => true),
 633 |     delay(5000).then(() => false),
 634 |   ]);
 635 |   assert(exited, 'the language server exited after deactivate');
 636 |   fs.rmSync(repo, { recursive: true, force: true });
 637 | }
 638 |
 639 | run().then(() => {
 640 |   console.log('ok node integration');
 641 | }).catch((error) => {
 642 |   console.error(error && error.stack ? error.stack : error);
 643 |   for (const client of instances) {
 644 |     if (client.stderr) console.error(`--- magit-lsp stderr ---\n${client.stderr}`);
 645 |   }
 646 |   for (const message of vscode.__test.messages) {
 647 |     console.error(`--- ${message.level}: ${message.text}`);
 648 |   }
 649 |   for (const line of vscode.__test.output) {
 650 |     console.error(`--- output: ${line}`);
 651 |   }
 652 |   process.exitCode = 1;
 653 | });
```

### Native launch, environment, CDP, key/dialog handling

### `clients/vscode/test/e2e/run.mjs` lines 1-36 of 218
```text
   1 | import { existsSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
   2 | import { execFileSync, spawnSync, spawn } from 'node:child_process';
   3 | import { resolve } from 'node:path';
   4 | import { createRequire } from 'node:module';
   5 | import { createServer } from 'node:net';
   6 |
   7 | /**
   8 |  * Optional real-editor suite: launch a unique Extension Development Host and
   9 |  * run `suite.js` inside it. The suite is skipped, not failed, when no editor
  10 |  * binary or display is available, so `make vscode-test` stays portable.
  11 |  */
  12 | const root = resolve(import.meta.dirname, '../../../..');
  13 | const candidates = [process.env.MAGIT_VSCODE_BIN, '/usr/bin/codium', '/usr/bin/code']
  14 |   .filter(Boolean);
  15 | const editor = candidates.find((candidate) => existsSync(candidate));
  16 | const focusedEdit = process.env.MAGIT_VSCODE_EDIT_ONLY === '1';
  17 | const focusedHead = process.env.MAGIT_VSCODE_HEAD_ONLY === '1';
  18 | if (!editor) {
  19 |   console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: no \`code\`/\`codium\` binary found (set MAGIT_VSCODE_BIN to run it)`);
  20 |   process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
  21 | }
  22 | if (!process.env.DISPLAY) {
  23 |   console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: DISPLAY is required for the real-editor keyboard checks`);
  24 |   process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
  25 | }
  26 | if (!existsSync(`${root}/target/debug/magit-lsp`)) {
  27 |   console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: missing target/debug/magit-lsp; run cargo build first`);
  28 |   process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
  29 | }
  30 |
  31 | // This subprocess observes the isolated renderer; it never sets context keys or
  32 | // dispatches commands. Image capture methods are explicitly forbidden.
  33 | if (process.argv[2] === '--cdp') {
  34 |   const [, , , port, method, params] = process.argv;
  35 |   if (/image|screenshot|snapshot/i.test(method)) {
  36 |     throw new Error(`Image-related CDP method is forbidden: ${method}`);
```
### `clients/vscode/test/e2e/run.mjs` lines 60-100 of 218
```text
  60 | const mirror = createRequire(import.meta.url)('../../lib/foldkeys.js').mapFoldKeys([
  61 |   { key: 'ctrl+alt+]', command: 'editor.toggleFold' },
  62 | ]);
  63 | mkdirSync(`${directory}/user/User`, { recursive: true });
  64 | // XTEST sends keysyms, while this desktop may use Colemak (KeyT maps to G in
  65 | // VSCode's scan-code dispatcher). Resolve the actual keyCode in this isolated
  66 | // profile, without changing the user's XKB layout or any Magit context/binding.
  67 | writeFileSync(`${directory}/user/User/settings.json`, JSON.stringify({
  68 |   'keyboard.dispatch': 'keyCode',
  69 |   'window.dialogStyle': 'custom',
  70 | }));
  71 | writeFileSync(`${directory}/user/User/keybindings.json`, JSON.stringify(mirror, null, 2));
  72 | writeFileSync(`${directory}/mirrored-bindings.json`, JSON.stringify(mirror, null, 2));
  73 | mkdirSync(`${repo}/.vscode`, { recursive: true });
  74 | execFileSync('git', ['init', '-q', repo]);
  75 | writeFileSync(`${repo}/tracked.txt`, 'original\n');
  76 | execFileSync('git', ['-C', repo, 'add', '.']);
  77 | execFileSync('git', ['-C', repo, '-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '-qm', 'initial']);
  78 | writeFileSync(`${repo}/tracked.txt`, 'replacement\n');
  79 | writeFileSync(`${repo}/.vscode/settings.json`, JSON.stringify({
  80 |   'magit-lsp.serverPath': `${root}/target/debug/magit-lsp`,
  81 |   ...(process.env.MAGIT_VSCODE_A33_ONLY === '1' ? { 'magit-lsp.profile': 'vim' } : {}),
  82 |   'security.workspace.trust.enabled': false,
  83 |   'workbench.colorTheme': 'Default Dark Modern',
  84 |   'window.zoomLevel': 0,
  85 |   'window.title': directory,
  86 | }));
  87 | // Keep editor subprocesses free of giant inherited shell/function variables.
  88 | const env = Object.fromEntries(['HOME', 'PATH', 'DISPLAY', 'XAUTHORITY',
  89 |   'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'LANG', 'USER', 'SHELL']
  90 |   .filter((key) => process.env[key]).map((key) => [key, process.env[key]]));
  91 | if (process.env.MAGIT_VSCODE_A06_ONLY === '1') env.MAGIT_VSCODE_A06_ONLY = '1';
  92 | if (process.env.MAGIT_VSCODE_A29_ONLY === '1') env.MAGIT_VSCODE_A29_ONLY = '1';
  93 | if (process.env.MAGIT_VSCODE_A33_ONLY === '1') env.MAGIT_VSCODE_A33_ONLY = '1';
  94 | if (process.env.MAGIT_VSCODE_A34_ONLY === '1') env.MAGIT_VSCODE_A34_ONLY = '1';
  95 | if (focusedEdit) env.MAGIT_VSCODE_EDIT_ONLY = '1';
  96 | env.MAGIT_VSCODE_EVIDENCE = directory;
  97 | const listener = createServer();
  98 | await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  99 | env.MAGIT_VSCODE_CDP_PORT = String(listener.address().port);
 100 | await new Promise((resolve) => listener.close(resolve));
```
### `clients/vscode/test/e2e/run.mjs` lines 108-180 of 218
```text
 108 | function ownedPids(base) {
 109 |   try {
 110 |     return execFileSync('pgrep', ['-f', '--', `${base}/user`])
 111 |       .toString().trim().split('\n').filter(Boolean).map(Number);
 112 |   } catch {
 113 |     return [];
 114 |   }
 115 | }
 116 |
 117 | /** Terminate only editor processes launched by this script. */
 118 | function stopOwned(base) {
 119 |   for (const pid of ownedPids(base)) {
 120 |     try {
 121 |       process.kill(pid, 'SIGKILL');
 122 |     } catch {
 123 |       // Already gone.
 124 |     }
 125 |   }
 126 | }
 127 |
 128 | // Extension test hosts suppress native dirty-close dialogs. A tiny normal-host
 129 | // launcher runs the same production suite without extensionTestsPath instead.
 130 | if (focusedEdit || focusedHead) {
 131 |   mkdirSync(`${directory}/launcher`);
 132 |   writeFileSync(`${directory}/launcher/package.json`, JSON.stringify({
 133 |     publisher: 'test', name: 'native-edit-launcher', version: '1.0.0',
 134 |     engines: { vscode: '^1.85.0' }, main: './launcher.js', activationEvents: ['*'],
 135 |   }));
 136 |   writeFileSync(`${directory}/launcher/launcher.js`, `
 137 |     /** Run production acceptance in a normal extension host. */
 138 |     exports.activate = function () {
 139 |       setImmediate(async () => {
 140 |         let status = 0;
 141 |         try { await require(${JSON.stringify(`${root}/clients/vscode/test/e2e/suite.js`)}).run(); }
 142 |         catch (error) { status = 1; console.error(error); }
 143 |         require('node:fs').writeFileSync(${JSON.stringify(`${directory}/native-complete.json`)}, JSON.stringify({ status }));
 144 |       });
 145 |     };
 146 |   `);
 147 | }
 148 | if (focusedHead) env.MAGIT_VSCODE_HEAD_ONLY = '1';
 149 | const args = [
 150 |   '--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes',
 151 |   `--remote-debugging-port=${env.MAGIT_VSCODE_CDP_PORT}`,
 152 |   '--disable-workspace-trust', '--user-data-dir', `${directory}/user`,
 153 |   '--extensions-dir', `${directory}/extensions`,
 154 |   `--extensionDevelopmentPath=${root}/clients/vscode`,
 155 |   ...(focusedEdit || focusedHead ? [`--extensionDevelopmentPath=${directory}/launcher`]
 156 |     : [`--extensionTestsPath=${root}/clients/vscode/test/e2e/suite.js`]), repo,
 157 | ];
 158 | let result;
 159 | if (focusedEdit || focusedHead) {
 160 |   const child = spawn(editor, args, { env, stdio: 'inherit' });
 161 |   let launchError;
 162 |   child.on('error', (error) => { launchError = error; });
 163 |   const deadline = Date.now() + 180000;
 164 |   await new Promise((resolve) => {
 165 |     const timer = setInterval(() => {
 166 |       if (launchError || child.exitCode !== null || existsSync(`${directory}/native-complete.json`)
 167 |         || Date.now() > deadline) { clearInterval(timer); resolve(); }
 168 |     }, 30);
 169 |   });
 170 |   result = { status: existsSync(`${directory}/native-complete.json`)
 171 |     ? JSON.parse((await import('node:fs')).readFileSync(`${directory}/native-complete.json`)).status : 1,
 172 |   error: launchError };
 173 | } else {
 174 |   result = spawnSync(editor, args, { env, stdio: 'inherit', timeout: 180000 });
 175 | }
 176 | stopOwned(directory);
 177 | console.log(`VSCode text evidence: ${directory}`);
 178 | if (result.error) console.error(result.error);
 179 | if (existsSync(`${directory}/failure.txt`)) {
 180 |   console.error(await (await import('node:fs/promises')).readFile(`${directory}/failure.txt`, 'utf8'));
```
### `clients/vscode/test/e2e/run.mjs` lines 186-218 of 218
```text
 186 | const evidenceReady = focusedHead ? headEvidence?.status === 'pass' && headEvidence.cases?.length === 8 : focusedEdit
 187 |   ? existsSync(`${directory}/real-file-native-evidence.json`)
 188 |   : process.env.MAGIT_VSCODE_A34_ONLY === '1'
 189 |   ? existsSync(`${directory}/a34-evidence.json`)
 190 |   : process.env.MAGIT_VSCODE_A33_ONLY === '1'
 191 |   ? existsSync(`${directory}/a33-evidence.json`)
 192 |   : process.env.MAGIT_VSCODE_A29_ONLY === '1'
 193 |   ? existsSync(`${directory}/a29-evidence.json`)
 194 |   : process.env.MAGIT_VSCODE_A06_ONLY === '1'
 195 |   ? existsSync(`${directory}/a06-evidence.json`)
 196 |   : existsSync(`${directory}/r-vs-2-evidence.json`)
 197 |     && existsSync(`${directory}/r-vs-3-evidence.json`)
 198 |     && existsSync(`${directory}/r-vs-4-evidence.json`)
 199 |     && existsSync(`${directory}/r-vs-5-evidence.json`)
 200 |     && existsSync(`${directory}/a06-evidence.json`);
 201 | if (!evidenceReady) {
 202 |   console.error(
 203 |     focusedHead
 204 |       ? 'Missing complete native HEAD operations evidence; see the isolated evidence directory above.'
 205 |       : focusedEdit
 206 |       ? 'Missing production real-file native evidence; see the isolated evidence directory above.'
 207 |       : process.env.MAGIT_VSCODE_A34_ONLY === '1'
 208 |       ? 'Missing A34 evidence; see the isolated evidence directory above.'
 209 |       : process.env.MAGIT_VSCODE_A33_ONLY === '1'
 210 |       ? 'Missing A33 evidence; see the isolated evidence directory above.'
 211 |       : process.env.MAGIT_VSCODE_A29_ONLY === '1'
 212 |       ? 'Missing A29 evidence; see the isolated evidence directory above.'
 213 |       : process.env.MAGIT_VSCODE_A06_ONLY === '1'
 214 |       ? 'Missing A06 evidence; see the isolated evidence directory above.'
 215 |       : 'Missing R-VS-2/R-VS-3/R-VS-4/R-VS-5/A06 evidence; see the isolated evidence directory above.'
 216 |   );
 217 | }
 218 | process.exit(result.status === 0 && evidenceReady ? 0 : 1);
```
### `clients/vscode/test/e2e/real-file-lifecycle.mjs` lines 1-20 of 130
```text
   1 | import assert from 'node:assert/strict';
   2 | import { existsSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, appendFileSync } from 'node:fs';
   3 | import { spawn, execFileSync } from 'node:child_process';
   4 | import { resolve } from 'node:path';
   5 | import { fileURLToPath } from 'node:url';
   6 | import { createServer } from 'node:net';
   7 |
   8 | const root = resolve(fileURLToPath(new URL('../../../..', import.meta.url)));
   9 | const editor = process.env.MAGIT_VSCODE_BIN;
  10 | assert(editor && existsSync(editor) && process.env.DISPLAY && existsSync(`${root}/target/debug/magit-lsp`), 'MAGIT_VSCODE_BIN, DISPLAY and built server required');
  11 | const directory = mkdtempSync('/tmp/opencode/real-file-lifecycle-');
  12 | console.log(`Evidence: ${directory}`);
  13 | const json = path => JSON.parse(readFileSync(path, 'utf8'));
  14 | const lines = path => existsSync(path) ? readFileSync(path, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
  15 | const write = (path, data) => writeFileSync(path, JSON.stringify(data, null, 2));
  16 | /** Poll observable state, surfacing durable host errors even after the host dies. */
  17 | async function wait(check, label, base, timeout = 90000) {
  18 |   const deadline = Date.now() + timeout;
  19 |   while (true) {
  20 |     if (base && existsSync(`${base}/failure.txt`)) throw Error(readFileSync(`${base}/failure.txt`, 'utf8'));
```
### `clients/vscode/test/e2e/real-file-lifecycle.mjs` lines 45-70 of 130
```text
  45 |     writeFileSync(`${repo}/tracked.txt`, `staged ${action}\n`); git('add', 'tracked.txt');
  46 |     writeFileSync(`${repo}/tracked.txt`, `unstaged ${action}\n`);
  47 |     const snapshot = () => ({ head: git('rev-parse', 'HEAD'), index: git('write-tree'), worktree: git('diff', '--binary'), status: git('status', '--porcelain') });
  48 |     const before = snapshot();
  49 |     mkdirSync(`${base}/other-workspace`); mkdirSync(`${base}/launcher`); mkdirSync(`${base}/user/User`, { recursive: true });
  50 |     write(`${base}/user/User/settings.json`, { 'magit-lsp.serverPath': `${root}/target/debug/magit-lsp`, 'window.title': base, 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'security.workspace.trust.enabled': false, 'git.enabled': false });
  51 |     const listener = createServer();
  52 |     await new Promise(r => listener.listen(0, '127.0.0.1', r));
  53 |     const port = listener.address().port;
  54 |     await new Promise(r => listener.close(r));
  55 |     write(`${base}/launcher/package.json`, { publisher: 'test', name: 'native-lifecycle', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './launcher.js', activationEvents: ['*'] });
  56 |     writeFileSync(`${base}/launcher/launcher.js`, `exports.activate = () => { setImmediate(() => require(${JSON.stringify(`${root}/clients/vscode/test/e2e/real-file-lifecycle.js`)}).run(${JSON.stringify({ root, evidence: base, repo, action, port })}).catch(e => require('fs').writeFileSync(${JSON.stringify(`${base}/failure.txt`)}, e.stack))); }; exports.deactivate = () => require('fs').appendFileSync(${JSON.stringify(`${base}/host.jsonl`)}, JSON.stringify({event:'deactivate',pid:process.pid,time:Date.now()})+'\\n');`);
  57 |     const env = Object.fromEntries(['HOME', 'PATH', 'DISPLAY', 'XAUTHORITY', 'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'LANG', 'USER', 'SHELL'].filter(k => process.env[k]).map(k => [k, process.env[k]]));
  58 |     const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust', `--remote-debugging-port=${port}`, '--user-data-dir', `${base}/user`, '--extensions-dir', `${base}/extensions`, `--extensionDevelopmentPath=${root}/clients/vscode`, `--extensionDevelopmentPath=${base}/launcher`, repo];
  59 |     write(`${base}/command.json`, { editor, args });
  60 |     const owned = new Map();
  61 |     const census = () => {
  62 |       const all = processes();
  63 |       let changed = true;
  64 |       while (changed) {
  65 |         changed = false;
  66 |         for (const p of all) if ((p.command.includes(`${base}/user`) || owned.has(p.parent)) && !owned.has(p.pid)) { owned.set(p.pid, p); changed = true; }
  67 |       }
  68 |       return all.filter(p => owned.get(p.pid)?.start === p.start && p.state !== 'Z');
  69 |     };
  70 |     let watcher;
```
### `clients/vscode/test/e2e/real-file-spike.mjs` lines 40-65 of 182
```text
  40 |     const record = data => writeFileSync(`${dir}/supervisor.jsonl`, JSON.stringify({ timestamp: new Date().toISOString(), ...data }) + '\n', { flag: 'a' });
  41 |     execFileSync('git', ['init', '-q', `${dir}/repo`]);
  42 |     writeFileSync(`${dir}/repo/.git/COMMIT_EDITMSG`, 'spike message\n');
  43 |     mkdirSync(`${dir}/other-workspace`);
  44 |     mkdirSync(`${dir}/extension`);
  45 |     writeFileSync(`${dir}/extension/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));
  46 |     copyFileSync(`${root}/clients/vscode/test/e2e/real-file-spike.js`, `${dir}/extension/extension.js`);
  47 |     mkdirSync(`${dir}/user/User`, { recursive: true });
  48 |     writeFileSync(`${dir}/user/User/settings.json`, JSON.stringify({ 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'window.title': dir, 'git.enabled': false }));
  49 |     const server = createServer();
  50 |     await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  51 |     const port = server.address().port;
  52 |     await new Promise(resolve => server.close(resolve));
  53 |     const env = { ...process.env, MAGIT_REAL_FILE_ROOT: root, MAGIT_REAL_FILE_EVIDENCE: dir,
  54 |       MAGIT_REAL_FILE_CDP: String(port), MAGIT_REAL_FILE_LIFECYCLE: action };
  55 |     delete env.ELECTRON_RUN_AS_NODE;
  56 |     const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust',
  57 |       `--remote-debugging-port=${port}`, '--user-data-dir', `${dir}/user`, '--extensions-dir', `${dir}/extensions`,
  58 |       `--extensionDevelopmentPath=${dir}/extension`, `${dir}/repo`];
  59 |     writeFileSync(`${dir}/command.json`, JSON.stringify({ editor, args, versions: execFileSync(editor, ['--version'], { encoding: 'utf8' }) }, null, 2));
  60 |     console.log(`Lifecycle ${action}: ${dir}`);
  61 |     let child, watcher;
  62 |     try {
  63 |       child = spawn(editor, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
  64 |       child.stdout.on('data', data => writeFileSync(`${dir}/stdout.txt`, data, { flag: 'a' }));
  65 |       child.stderr.on('data', data => writeFileSync(`${dir}/stderr.txt`, data, { flag: 'a' }));
```
### `clients/vscode/test/e2e/real-file-spike.mjs` lines 116-145 of 182
```text
 116 | const dir = mkdtempSync('/tmp/opencode/real-file-vscode-');
 117 | const repo = `${dir}/repo`;
 118 | execFileSync('git', ['init', '-q', repo]);
 119 | writeFileSync(`${repo}/.git/COMMIT_EDITMSG`, 'spike message\n');
 120 | const ext = `${dir}/extension`; mkdirSync(ext);
 121 | writeFileSync(`${ext}/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));
 122 | copyFileSync(`${root}/clients/vscode/test/e2e/real-file-spike.js`, `${ext}/extension.js`);
 123 | mkdirSync(`${dir}/user/User`, { recursive: true });
 124 | writeFileSync(`${dir}/user/User/settings.json`, JSON.stringify({ 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'window.title': dir, 'git.enabled': false }));
 125 | const listener = createServer();
 126 | await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
 127 | const port = listener.address().port;
 128 | await new Promise(resolve => listener.close(resolve));
 129 | const env = { ...process.env, MAGIT_REAL_FILE_ROOT: root, MAGIT_REAL_FILE_EVIDENCE: dir, MAGIT_REAL_FILE_CDP: String(port) };
 130 | const native = process.argv.includes('--native');
 131 | if (native) env.MAGIT_REAL_FILE_NATIVE = '1';
 132 | delete env.ELECTRON_RUN_AS_NODE;
 133 | const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust',
 134 |   `--remote-debugging-port=${port}`, '--user-data-dir', `${dir}/user`, '--extensions-dir', `${dir}/extensions`,
 135 |   `--extensionDevelopmentPath=${ext}`, ...(!native ? [`--extensionTestsPath=${root}/clients/vscode/test/e2e/real-file-spike.js`] : []), repo];
 136 | writeFileSync(`${dir}/command.json`, JSON.stringify({ editor, args, versions: execFileSync(editor, ['--version'], { encoding: 'utf8' }) }, null, 2));
 137 | let result;
 138 | try {
 139 | if (native) {
 140 |   const child = spawn(editor, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
 141 |   let stdout = '', stderr = '';
 142 |   let launchError;
 143 |   child.on('error', error => { launchError = error; });
 144 |   child.stdout.on('data', data => { stdout += data; });
 145 |   child.stderr.on('data', data => { stderr += data; });
```
### `clients/vscode/test/e2e/real-file-spike.js` lines 1-25 of 316
```text
   1 | 'use strict';
   2 | // Isolated test extension and extension-host suite; never loads production code.
   3 | const vscode = require('vscode');
   4 | const fs = require('fs');
   5 | const { execFileSync } = require('child_process');
   6 | const assert = require('node:assert/strict');
   7 | const base = process.env.MAGIT_REAL_FILE_EVIDENCE;
   8 | const root = process.env.MAGIT_REAL_FILE_ROOT;
   9 | const trace = () => fs.existsSync(`${base}/wire.jsonl`)
  10 |   ? fs.readFileSync(`${base}/wire.jsonl`, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
  11 | const count = (method) => trace().filter(m => m.method === method).length;
  12 |
  13 | /** Await a condition with a bounded deadline, not a fixed settling delay. */
  14 | function until(check, label, timeout = 5000) {
  15 |   return new Promise((resolve, reject) => {
  16 |     const deadline = Date.now() + timeout;
  17 |     const timer = setInterval(() => {
  18 |       try {
  19 |         const result = check();
  20 |         if (result) { clearInterval(timer); resolve(result); }
  21 |         else if (Date.now() >= deadline) { clearInterval(timer); reject(new Error(`Deadline: ${label}`)); }
  22 |       } catch (error) { clearInterval(timer); reject(error); }
  23 |     }, 25);
  24 |   });
  25 | }
```
### `clients/vscode/test/e2e/real-file-spike.js` lines 115-155 of 316
```text
 115 |     // still proved externally by exit/reactivation, never by this rejection.
 116 |     log({ event: 'lifecycle-command-rejected', command, error: error.stack });
 117 |   }
 118 | }
 119 |
 120 | /** Exercise the real editor, retaining observations even when the hard gate fails. */
 121 | exports.run = async function run() {
 122 |   const result = { gate: 'incomplete', cases: {} };
 123 |   let client;
 124 |   const cdp = (method, params = {}) => JSON.parse(execFileSync(process.execPath, [
 125 |     `${root}/clients/vscode/test/e2e/real-file-spike.mjs`, '--cdp', process.env.MAGIT_REAL_FILE_CDP, method, JSON.stringify(params),
 126 |   ], { env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' }, encoding: 'utf8', timeout: 10000 }));
 127 |   try {
 128 |     ({ client } = await vscode.extensions.getExtension('spike.real-file-spike').activate());
 129 |     const windowId = await until(() => {
 130 |       try {
 131 |         const ids = execFileSync('xdotool', ['search', '--name', base], { encoding: 'utf8' }).trim().split('\n');
 132 |         return ids.length === 1 && ids[0];
 133 |       } catch (_) { return false; }
 134 |     }, 'isolated window');
 135 |     try { execFileSync('bspc', ['node', windowId, '-t', 'floating'], { stdio: 'ignore' }); } catch (_) { /* other WM */ }
 136 |     execFileSync('xdotool', ['windowsize', '--sync', windowId, '1200', '760']);
 137 |     await until(() => cdp('Runtime.evaluate', { expression: 'innerWidth >= 1100' }).result.value, 'window geometry');
 138 |     const uri = vscode.Uri.file(`${base}/repo/.git/COMMIT_EDITMSG`);
 139 |     const api = () => fs.existsSync(`${base}/api.jsonl`)
 140 |       ? fs.readFileSync(`${base}/api.jsonl`, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
 141 |     const qualifiedClosures = () => api().filter(item => item.event === 'tabs' && item.qualified);
 142 |     const matchingTabs = () => vscode.window.tabGroups.all.flatMap(g => g.tabs)
 143 |       .filter(tab => tab.input?.uri?.toString() === uri.toString());
 144 |     const show = () => client.sendRequest('spike/show', { uri: uri.toString(), takeFocus: true });
 145 |     result.cases.show = await show();
 146 |     assert.equal(result.cases.show.success, true);
 147 |     assert.equal(api().filter(item => item.event === 'showDocument').length, 1, 'real middleware invoked');
 148 |     result.cases.preview = { isPreview: vscode.window.tabGroups.all.flatMap(g => g.tabs)
 149 |       .find(tab => tab.input && tab.input.uri && tab.input.uri.toString() === uri.toString()).isPreview };
 150 |     if (result.cases.preview.isPreview) throw new Error('showDocument opened preview tab');
 151 |     await until(() => count('textDocument/didOpen'), 'wire didOpen');
 152 |     const doc = vscode.window.activeTextEditor.document;
 153 |     result.cases.language = doc.languageId;
 154 |     await until(() => cdp('Runtime.evaluate', { expression: 'document.body.innerText' }).result.value.replace(/\s/g, ' ').includes('spike message'), 'painted editor');
 155 |     const otherUri = vscode.Uri.file(`${base}/repo/other.txt`);
```

### E2E exported functions and named cases (declaration/case source lines)


clients/vscode/test/e2e/head-operations.js:
`clients/vscode/test/e2e/head-operations.js:9: exports.run = async function run({ extension, evidence, repo, cdp, waitFor, checkpoint,`

clients/vscode/test/e2e/real-file-lifecycle.js:
`clients/vscode/test/e2e/real-file-lifecycle.js:8: exports.run = async function run({ root, evidence, repo, action, port }) {`

clients/vscode/test/e2e/real-file-lifecycle.mjs:
`clients/vscode/test/e2e/real-file-lifecycle.mjs:17: async function wait(check, label, base, timeout = 90000) {`
`clients/vscode/test/e2e/real-file-lifecycle.mjs:28: function processes() {`
`clients/vscode/test/e2e/real-file-lifecycle.mjs:55: write(`${base}/launcher/package.json`, { publisher: 'test', name: 'native-lifecycle', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './launcher.js', activationEvents: ['*'] });`
`clients/vscode/test/e2e/real-file-lifecycle.mjs:56: writeFileSync(`${base}/launcher/launcher.js`, `exports.activate = () => { setImmediate(() => require(${JSON.stringify(`${root}/clients/vscode/test/e2e/real-file-lifecycle.js`)}).run(${JSON.stringify({ root, evidence: base, repo, action, port })}).catch(e => require('fs').writeFileSync(${JSON.stringify(`${base}/failure.txt`)}, e.stack))); }; exports.deactivate = () => require('fs').appendFileSync(${JSON.stringify(`${base}/host.jsonl`)}, JSON.stringify({event:'deactivate',pid:process.pid,time:Date.now()})+'\\n');`);`

clients/vscode/test/e2e/real-file-native.js:
`clients/vscode/test/e2e/real-file-native.js:9: exports.run = async function run({ extension, evidence, repo, cdp, waitFor, checkpoint,`
`clients/vscode/test/e2e/real-file-native.js:76: cases.push({ name: 'fresh-run', runId: record.runId, initial });`
`clients/vscode/test/e2e/real-file-native.js:97: cases.push({ name: 'committed', runId: record.runId, before, after: head(), message: message() });`
`clients/vscode/test/e2e/real-file-native.js:151: cases.push({ name: 'native-cancel', head: head(), dirty: true, tabs: 1 });`
`clients/vscode/test/e2e/real-file-native.js:169: cases.push({ name: 'discard-preserves-disk-over-unsaved-sync', saved: message() });`
`clients/vscode/test/e2e/real-file-native.js:193: cases.push({ name: 'wrong-version-protocol-probe', runId: record.runId,`
`clients/vscode/test/e2e/real-file-native.js:212: cases.push({ name: 'unsafe-before-native-close', ...ownership(record) });`
`clients/vscode/test/e2e/real-file-native.js:232: cases.push({ name: 'unsafe-native-close-rejected', error: record.syncError,`
`clients/vscode/test/e2e/real-file-native.js:248: cases.push({ name: 'unsafe-restored-before-native-retry', ...ownership(record) });`
`clients/vscode/test/e2e/real-file-native.js:400: capabilities: {}, clientInfo: { name: 'R-EDIT-REAL semantic owner probe' } });`
`clients/vscode/test/e2e/real-file-native.js:417: cases.push({ name: 'two-repository-cross-wire-semantic-protocol-probes', roots: { A: repo, B: secondRepo },`
`clients/vscode/test/e2e/real-file-native.js:440: cases.push({ name: 'two-repository-native-A-last-close-B-pending', before: beforeProbes,`
`clients/vscode/test/e2e/real-file-native.js:449: cases.push({ name: 'two-repository-native-A-close-abort-B', beforeAbort,`
`clients/vscode/test/e2e/real-file-native.js:459: cases.push({ name: 'two-repository-late-native-B-close-after-abort', before: beforeAbort,`

clients/vscode/test/e2e/real-file-spike.js:
`clients/vscode/test/e2e/real-file-spike.js:14: function until(check, label, timeout = 5000) {`
`clients/vscode/test/e2e/real-file-spike.js:28: exports.activate = async function activate(context) {`
`clients/vscode/test/e2e/real-file-spike.js:73: if (process.env.MAGIT_REAL_FILE_NATIVE === '1') setImmediate(() => exports.run().catch(error => console.error(error)));`
`clients/vscode/test/e2e/real-file-spike.js:78: exports.deactivate = function deactivate() {`
`clients/vscode/test/e2e/real-file-spike.js:83: async function observeLifecycle(client) {`
`clients/vscode/test/e2e/real-file-spike.js:121: exports.run = async function run() {`

clients/vscode/test/e2e/real-file-spike.mjs:
`clients/vscode/test/e2e/real-file-spike.mjs:25: async function lifecycle() {`
`clients/vscode/test/e2e/real-file-spike.mjs:45: writeFileSync(`${dir}/extension/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));`
`clients/vscode/test/e2e/real-file-spike.mjs:121: writeFileSync(`${ext}/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));`

clients/vscode/test/e2e/run.mjs:
`clients/vscode/test/e2e/run.mjs:108: function ownedPids(base) {`
`clients/vscode/test/e2e/run.mjs:118: function stopOwned(base) {`
`clients/vscode/test/e2e/run.mjs:133: publisher: 'test', name: 'native-edit-launcher', version: '1.0.0',`
`clients/vscode/test/e2e/run.mjs:138: exports.activate = function () {`

clients/vscode/test/e2e/suite.js:
`clients/vscode/test/e2e/suite.js:21: function settle(milliseconds) {`
`clients/vscode/test/e2e/suite.js:32: async function waitFor(condition, label, timeout = 15000) {`
`clients/vscode/test/e2e/suite.js:47: function workspaceFiles(root) {`
`clients/vscode/test/e2e/suite.js:62: async function run() {`
`clients/vscode/test/e2e/suite.js:1721: cases.push({ name: 'native-save-normal-close', message: commitMessage, head: head() });`
`clients/vscode/test/e2e/suite.js:1744: cases.push({ name: 'version-recovery-confirm', head: head(),`
`clients/vscode/test/e2e/suite.js:1780: cases.push({ name: 'native-last-tab-saved-file-close', head: head() });`
`clients/vscode/test/e2e/suite.js:1792: cases.push({ name: 'progress-cancel', token, head: head() });`

### Explicit findings / limits
`window.dialogStyle` search: `rg -n "window\.dialogStyle" clients/vscode/test/e2e`; actual settings are shown in `run.mjs` (line 69) and lifecycle launcher (line 50); they set custom dialog style. Optional xvfb/CDP Save/Don't Save DOM experiment: NOT ATTEMPTED. `rg -n 'view/open|status/repo|viewId.*status|textDocument/didOpen' build/test-logs/vscode.*` returned no matches; existing build logs did not yield a real VSCode client→server status-open wire excerpt. `wire.jsonl`/`api.jsonl` references in spike sources are a configured/optional evidence path, not captured evidence found here. Actual trace excerpt: NOT FOUND. Do not infer from fixtures.


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
## D. Duplicated runtime tables

Commands: `nl -ba lua/magit-lsp/keys.lua`; `nl -ba lua/magit-lsp/highlights.lua`; `nl -ba lua/magit-lsp/prompt.lua`; `nl -ba lua/magit-lsp/config.lua`; `nl -ba lua/magit-lsp/effects.lua`; `nl -ba clients/vscode/lib/menu.js`; `nl -ba clients/vscode/lib/foldkeys.js`; `nl -ba clients/vscode/lib/semantic.js`; `nl -ba clients/vscode/package.json`; `nl -ba clients/vscode/lib/prompt.js`; `nl -ba clients/vscode/lib/requests.js`; `nl -ba clients/vscode/lib/effects.js`; `nl -ba clients/vscode/lib/session.js`; `nl -ba clients/vscode/scripts/gen-keybindings.mjs`. Excerpts below retain exact source text and line numbers.

### Key translation / delivery

Neovim canonical↔lhs translator is `keys.lua` (320 lines; full public implementation excerpt, including all named-key/modifier translation helpers):

```lua
lua/magit-lsp/keys.lua:1: --- Translate between the server's canonical key notation (TASKS.md §3.8) and
lua/magit-lsp/keys.lua:2: --- Neovim's key notation. Pure functions: no buffer or editor state.
lua/magit-lsp/keys.lua:3: ---
lua/magit-lsp/keys.lua:4: --- The canonical form is `[modifier+]*key` with space-separated chord steps.
lua/magit-lsp/keys.lua:5: --- Neovim's form is a single `lhs` string, e.g. `ctrl+c` → `<C-c>` and
lua/magit-lsp/keys.lua:6: --- `g r` → `gr`.
lua/magit-lsp/keys.lua:7:
lua/magit-lsp/keys.lua:8: local M = {}
lua/magit-lsp/keys.lua:9:
lua/magit-lsp/keys.lua:10: --- Canonical named keys and their Neovim spellings.
lua/magit-lsp/keys.lua:11: local NAMED = {
lua/magit-lsp/keys.lua:12:   Enter = 'CR',
lua/magit-lsp/keys.lua:13:   Escape = 'Esc',
lua/magit-lsp/keys.lua:14:   Tab = 'Tab',
lua/magit-lsp/keys.lua:15:   Backspace = 'BS',
lua/magit-lsp/keys.lua:16:   Delete = 'Del',
lua/magit-lsp/keys.lua:17:   Insert = 'Insert',
lua/magit-lsp/keys.lua:18:   Home = 'Home',
lua/magit-lsp/keys.lua:19:   End = 'End',
lua/magit-lsp/keys.lua:20:   PageUp = 'PageUp',
lua/magit-lsp/keys.lua:21:   PageDown = 'PageDown',
lua/magit-lsp/keys.lua:22:   ArrowUp = 'Up',
lua/magit-lsp/keys.lua:23:   ArrowDown = 'Down',
lua/magit-lsp/keys.lua:24:   ArrowLeft = 'Left',
lua/magit-lsp/keys.lua:25:   ArrowRight = 'Right',
lua/magit-lsp/keys.lua:26: }
lua/magit-lsp/keys.lua:27:
lua/magit-lsp/keys.lua:28: for index = 1, 12 do
lua/magit-lsp/keys.lua:29:   NAMED['F' .. index] = 'F' .. index
lua/magit-lsp/keys.lua:30: end
lua/magit-lsp/keys.lua:31:
lua/magit-lsp/keys.lua:32: local NAMED_FROM_NVIM = {}
lua/magit-lsp/keys.lua:33: for canonical, nvim in pairs(NAMED) do
lua/magit-lsp/keys.lua:34:   NAMED_FROM_NVIM[nvim] = canonical
lua/magit-lsp/keys.lua:35: end
lua/magit-lsp/keys.lua:36:
lua/magit-lsp/keys.lua:37: --- Canonical modifier order (§3.8) and the Neovim letter for each.
lua/magit-lsp/keys.lua:38: local MODIFIER_ORDER = { 'ctrl', 'alt', 'shift', 'meta' }
lua/magit-lsp/keys.lua:39: local MODIFIER_LETTER = { ctrl = 'C', alt = 'A', shift = 'S', meta = 'M' }
lua/magit-lsp/keys.lua:40: local LETTER_MODIFIER = { C = 'ctrl', A = 'alt', S = 'shift', M = 'meta' }
lua/magit-lsp/keys.lua:41: local WORD_MODIFIER = {
lua/magit-lsp/keys.lua:42:   ctrl = 'ctrl',
lua/magit-lsp/keys.lua:43:   control = 'ctrl',
lua/magit-lsp/keys.lua:44:   alt = 'alt',
lua/magit-lsp/keys.lua:45:   shift = 'shift',
lua/magit-lsp/keys.lua:46:   meta = 'meta',
lua/magit-lsp/keys.lua:47: }
lua/magit-lsp/keys.lua:48:
lua/magit-lsp/keys.lua:49: local function fail(message)
lua/magit-lsp/keys.lua:50:   return nil, message
lua/magit-lsp/keys.lua:51: end
lua/magit-lsp/keys.lua:52:
lua/magit-lsp/keys.lua:53: --- The single character of `text`, or nil when it is not one character.
lua/magit-lsp/keys.lua:54: local function one_char(text)
lua/magit-lsp/keys.lua:55:   if vim.fn.strchars(text) ~= 1 then
lua/magit-lsp/keys.lua:56:     return nil
lua/magit-lsp/keys.lua:57:   end
lua/magit-lsp/keys.lua:58:   return vim.fn.strcharpart(text, 0, 1)
lua/magit-lsp/keys.lua:59: end
lua/magit-lsp/keys.lua:60:
lua/magit-lsp/keys.lua:61: local function split_ws(text)
lua/magit-lsp/keys.lua:62:   local out = {}
lua/magit-lsp/keys.lua:63:   for word in text:gmatch('%S+') do
lua/magit-lsp/keys.lua:64:     out[#out + 1] = word
lua/magit-lsp/keys.lua:65:   end
lua/magit-lsp/keys.lua:66:   return out
lua/magit-lsp/keys.lua:67: end
lua/magit-lsp/keys.lua:68:
lua/magit-lsp/keys.lua:69: --- Parse one canonical step into `{ ctrl, alt, shift, meta, name|char }`.
lua/magit-lsp/keys.lua:70: local function parse_step(text)
lua/magit-lsp/keys.lua:71:   if text == '' then
lua/magit-lsp/keys.lua:72:     return fail('empty key')
lua/magit-lsp/keys.lua:73:   end
lua/magit-lsp/keys.lua:74:   if text == 'Plus' then
lua/magit-lsp/keys.lua:75:     return { key = { char = '+' } }
lua/magit-lsp/keys.lua:76:   end
lua/magit-lsp/keys.lua:77:
lua/magit-lsp/keys.lua:78:   local parts = {}
lua/magit-lsp/keys.lua:79:   for part in (text .. '+'):gmatch('([^+]+)') do
lua/magit-lsp/keys.lua:80:     parts[#parts + 1] = part
lua/magit-lsp/keys.lua:81:   end
lua/magit-lsp/keys.lua:82:   if #parts == 0 then
lua/magit-lsp/keys.lua:83:     return fail('empty key')
lua/magit-lsp/keys.lua:84:   end
lua/magit-lsp/keys.lua:85:
lua/magit-lsp/keys.lua:86:   local step = {}
lua/magit-lsp/keys.lua:87:   for index = 1, #parts - 1 do
lua/magit-lsp/keys.lua:88:     local modifier = parts[index]
lua/magit-lsp/keys.lua:89:     if modifier == '' then
lua/magit-lsp/keys.lua:90:       return fail('empty modifier in key')
lua/magit-lsp/keys.lua:91:     end
lua/magit-lsp/keys.lua:92:     local name = WORD_MODIFIER[modifier]
lua/magit-lsp/keys.lua:93:     if not name then
lua/magit-lsp/keys.lua:94:       return fail('unknown modifier: ' .. modifier)
lua/magit-lsp/keys.lua:95:     end
lua/magit-lsp/keys.lua:96:     if step[name] then
lua/magit-lsp/keys.lua:97:       return fail('duplicate modifier: ' .. modifier)
lua/magit-lsp/keys.lua:98:     end
lua/magit-lsp/keys.lua:99:     step[name] = true
lua/magit-lsp/keys.lua:100:   end
lua/magit-lsp/keys.lua:101:
lua/magit-lsp/keys.lua:102:   local key = parts[#parts]
lua/magit-lsp/keys.lua:103:   if key == 'Space' then
lua/magit-lsp/keys.lua:104:     step.key = { char = ' ' }
lua/magit-lsp/keys.lua:105:   elseif key == 'Plus' then
lua/magit-lsp/keys.lua:106:     step.key = { char = '+' }
lua/magit-lsp/keys.lua:107:   elseif NAMED[key] then
lua/magit-lsp/keys.lua:108:     step.key = { name = key }
lua/magit-lsp/keys.lua:109:   else
lua/magit-lsp/keys.lua:110:     local char = one_char(key)
lua/magit-lsp/keys.lua:111:     if not char then
lua/magit-lsp/keys.lua:112:       return fail('unknown key: ' .. key)
lua/magit-lsp/keys.lua:113:     end
lua/magit-lsp/keys.lua:114:     if char == '+' then
lua/magit-lsp/keys.lua:115:       return fail('write the plus key as `Plus`')
lua/magit-lsp/keys.lua:116:     end
lua/magit-lsp/keys.lua:117:     if char:match('%s') then
lua/magit-lsp/keys.lua:118:       return fail('write the space key as `Space`')
lua/magit-lsp/keys.lua:119:     end
lua/magit-lsp/keys.lua:120:     if step.shift and not (step.ctrl or step.alt or step.meta) then
lua/magit-lsp/keys.lua:121:       return fail('`shift+' .. char .. '` is invalid; write the shifted character instead')
lua/magit-lsp/keys.lua:122:     end
lua/magit-lsp/keys.lua:123:     step.key = { char = char }
lua/magit-lsp/keys.lua:124:   end
lua/magit-lsp/keys.lua:125:
lua/magit-lsp/keys.lua:126:   return step
lua/magit-lsp/keys.lua:127: end
lua/magit-lsp/keys.lua:128:
lua/magit-lsp/keys.lua:129: --- The Neovim spelling of one parsed step.
lua/magit-lsp/keys.lua:130: local function step_to_lhs(step)
lua/magit-lsp/keys.lua:131:   local letters = {}
lua/magit-lsp/keys.lua:132:   for _, modifier in ipairs(MODIFIER_ORDER) do
lua/magit-lsp/keys.lua:133:     if step[modifier] then
lua/magit-lsp/keys.lua:134:       letters[#letters + 1] = MODIFIER_LETTER[modifier]
lua/magit-lsp/keys.lua:135:     end
lua/magit-lsp/keys.lua:136:   end
lua/magit-lsp/keys.lua:137:
lua/magit-lsp/keys.lua:138:   local key = step.key
lua/magit-lsp/keys.lua:139:   local name
lua/magit-lsp/keys.lua:140:   if key.name then
lua/magit-lsp/keys.lua:141:     name = NAMED[key.name]
lua/magit-lsp/keys.lua:142:     if not name then
lua/magit-lsp/keys.lua:143:       return fail('unknown key: ' .. key.name)
lua/magit-lsp/keys.lua:144:     end
lua/magit-lsp/keys.lua:145:   elseif key.char == ' ' then
lua/magit-lsp/keys.lua:146:     name = 'Space'
lua/magit-lsp/keys.lua:147:   elseif key.char == '+' then
lua/magit-lsp/keys.lua:148:     name = '+'
lua/magit-lsp/keys.lua:149:   elseif key.char == '<' then
lua/magit-lsp/keys.lua:150:     name = 'lt'
lua/magit-lsp/keys.lua:151:   else
lua/magit-lsp/keys.lua:152:     name = key.char
lua/magit-lsp/keys.lua:153:   end
lua/magit-lsp/keys.lua:154:
lua/magit-lsp/keys.lua:155:   if #letters == 0 then
lua/magit-lsp/keys.lua:156:     if key.name then
lua/magit-lsp/keys.lua:157:       return '<' .. name .. '>'
lua/magit-lsp/keys.lua:158:     end
lua/magit-lsp/keys.lua:159:     if key.char == ' ' then
lua/magit-lsp/keys.lua:160:       return '<Space>'
lua/magit-lsp/keys.lua:161:     end
lua/magit-lsp/keys.lua:162:     if key.char == '<' then
lua/magit-lsp/keys.lua:163:       return '<lt>'
lua/magit-lsp/keys.lua:164:     end
lua/magit-lsp/keys.lua:165:     return key.char
lua/magit-lsp/keys.lua:166:   end
lua/magit-lsp/keys.lua:167:
lua/magit-lsp/keys.lua:168:   return '<' .. table.concat(letters, '-') .. '-' .. name .. '>'
lua/magit-lsp/keys.lua:169: end
lua/magit-lsp/keys.lua:170:
lua/magit-lsp/keys.lua:171: --- Split a canonical chord into its server-provided step strings.
lua/magit-lsp/keys.lua:172: ---@param chord string
lua/magit-lsp/keys.lua:173: ---@return string[]|nil
lua/magit-lsp/keys.lua:174: function M.steps(chord)
lua/magit-lsp/keys.lua:175:   if type(chord) ~= 'string' then
lua/magit-lsp/keys.lua:176:     return nil
lua/magit-lsp/keys.lua:177:   end
lua/magit-lsp/keys.lua:178:   local steps = split_ws(chord)
lua/magit-lsp/keys.lua:179:   if #steps == 0 then
lua/magit-lsp/keys.lua:180:     return nil
lua/magit-lsp/keys.lua:181:   end
lua/magit-lsp/keys.lua:182:   return steps
lua/magit-lsp/keys.lua:183: end
lua/magit-lsp/keys.lua:184:
lua/magit-lsp/keys.lua:185: --- Translate a canonical chord to one Neovim `lhs`.
lua/magit-lsp/keys.lua:186: ---@param chord string
lua/magit-lsp/keys.lua:187: ---@return string|nil
lua/magit-lsp/keys.lua:188: function M.to_lhs(chord)
lua/magit-lsp/keys.lua:189:   local steps, err = M.steps(chord)
lua/magit-lsp/keys.lua:190:   if not steps then
lua/magit-lsp/keys.lua:191:     return nil, err
lua/magit-lsp/keys.lua:192:   end
lua/magit-lsp/keys.lua:193:   local out = {}
lua/magit-lsp/keys.lua:194:   for _, text in ipairs(steps) do
lua/magit-lsp/keys.lua:195:     local step = parse_step(text)
lua/magit-lsp/keys.lua:196:     local lhs, step_err = step_to_lhs(step)
lua/magit-lsp/keys.lua:197:     if not lhs then
lua/magit-lsp/keys.lua:198:       return nil, step_err
lua/magit-lsp/keys.lua:199:     end
lua/magit-lsp/keys.lua:200:     out[#out + 1] = lhs
lua/magit-lsp/keys.lua:201:   end
lua/magit-lsp/keys.lua:202:   return table.concat(out), steps
lua/magit-lsp/keys.lua:203: end
lua/magit-lsp/keys.lua:204:
lua/magit-lsp/keys.lua:205: --- Turn a Neovim special-key body such as `C-S-c` or `CR` into canonical.
lua/magit-lsp/keys.lua:206: local function special_to_canonical(special)
lua/magit-lsp/keys.lua:207:   local parts = {}
lua/magit-lsp/keys.lua:208:   for part in (special .. '-'):gmatch('([^-]*)-') do
lua/magit-lsp/keys.lua:209:     parts[#parts + 1] = part
lua/magit-lsp/keys.lua:210:   end
lua/magit-lsp/keys.lua:211:   if #parts == 0 then
lua/magit-lsp/keys.lua:212:     return fail('empty key')
lua/magit-lsp/keys.lua:213:   end
lua/magit-lsp/keys.lua:214:
lua/magit-lsp/keys.lua:215:   local key = parts[#parts]
lua/magit-lsp/keys.lua:216:   local modifiers = {}
lua/magit-lsp/keys.lua:217:   for index = 1, #parts - 1 do
lua/magit-lsp/keys.lua:218:     local part = parts[index]
lua/magit-lsp/keys.lua:219:     local modifier = LETTER_MODIFIER[part] or WORD_MODIFIER[part:lower()]
lua/magit-lsp/keys.lua:220:     if not modifier then
lua/magit-lsp/keys.lua:221:       return fail('unknown modifier: ' .. part)
lua/magit-lsp/keys.lua:222:     end
lua/magit-lsp/keys.lua:223:     if modifiers[modifier] then
lua/magit-lsp/keys.lua:224:       return fail('duplicate modifier: ' .. part)
lua/magit-lsp/keys.lua:225:     end
lua/magit-lsp/keys.lua:226:     modifiers[modifier] = true
lua/magit-lsp/keys.lua:227:   end
lua/magit-lsp/keys.lua:228:
lua/magit-lsp/keys.lua:229:   local canonical_key
lua/magit-lsp/keys.lua:230:   if key == 'lt' then
lua/magit-lsp/keys.lua:231:     canonical_key = '<'
lua/magit-lsp/keys.lua:232:   elseif key == 'Space' then
lua/magit-lsp/keys.lua:233:     canonical_key = 'Space'
lua/magit-lsp/keys.lua:234:   elseif key == 'Plus' then
lua/magit-lsp/keys.lua:235:     canonical_key = 'Plus'
lua/magit-lsp/keys.lua:236:   elseif NAMED_FROM_NVIM[key] then
lua/magit-lsp/keys.lua:237:     canonical_key = NAMED_FROM_NVIM[key]
lua/magit-lsp/keys.lua:238:   else
lua/magit-lsp/keys.lua:239:     local char = one_char(key)
lua/magit-lsp/keys.lua:240:     if not char then
lua/magit-lsp/keys.lua:241:       return fail('unknown key: ' .. key)
lua/magit-lsp/keys.lua:242:     end
lua/magit-lsp/keys.lua:243:     if char == '+' then
lua/magit-lsp/keys.lua:244:       canonical_key = 'Plus'
lua/magit-lsp/keys.lua:245:     elseif char:match('%s') then
lua/magit-lsp/keys.lua:246:       canonical_key = 'Space'
lua/magit-lsp/keys.lua:247:     else
lua/magit-lsp/keys.lua:248:       canonical_key = char
lua/magit-lsp/keys.lua:249:     end
lua/magit-lsp/keys.lua:250:   end
lua/magit-lsp/keys.lua:251:
lua/magit-lsp/keys.lua:252:   if type(canonical_key) ~= 'string' then
lua/magit-lsp/keys.lua:253:     return nil, canonical_key
lua/magit-lsp/keys.lua:254:   end
lua/magit-lsp/keys.lua:255:
lua/magit-lsp/keys.lua:256:   if canonical_key == '<' or canonical_key == 'Space' or canonical_key == 'Plus' or NAMED[canonical_key] then
lua/magit-lsp/keys.lua:257:     -- Named keys and the two spelled-out characters.
lua/magit-lsp/keys.lua:258:   elseif modifiers.shift and not (modifiers.ctrl or modifiers.alt or modifiers.meta) then
lua/magit-lsp/keys.lua:259:     return fail('shifted character keys have no `shift+` form')
lua/magit-lsp/keys.lua:260:   end
lua/magit-lsp/keys.lua:261:
lua/magit-lsp/keys.lua:262:   local out = {}
lua/magit-lsp/keys.lua:263:   for _, modifier in ipairs(MODIFIER_ORDER) do
lua/magit-lsp/keys.lua:264:     if modifiers[modifier] then
lua/magit-lsp/keys.lua:265:       out[#out + 1] = modifier
lua/magit-lsp/keys.lua:266:     end
lua/magit-lsp/keys.lua:267:   end
lua/magit-lsp/keys.lua:268:   out[#out + 1] = canonical_key
lua/magit-lsp/keys.lua:269:   return table.concat(out, '+')
lua/magit-lsp/keys.lua:270: end
lua/magit-lsp/keys.lua:271:
lua/magit-lsp/keys.lua:272: --- Translate one Neovim `lhs` to a canonical chord.
lua/magit-lsp/keys.lua:273: ---@param lhs string
lua/magit-lsp/keys.lua:274: ---@return string|nil
lua/magit-lsp/keys.lua:275: function M.from_lhs(lhs)
lua/magit-lsp/keys.lua:276:   if type(lhs) ~= 'string' then
lua/magit-lsp/keys.lua:277:     return nil, 'lhs must be a string'
lua/magit-lsp/keys.lua:278:   end
lua/magit-lsp/keys.lua:279:   if lhs == '' then
lua/magit-lsp/keys.lua:280:     return nil, 'empty lhs'
lua/magit-lsp/keys.lua:281:   end
lua/magit-lsp/keys.lua:282:
lua/magit-lsp/keys.lua:283:   local steps = {}
lua/magit-lsp/keys.lua:284:   local index = 1
lua/magit-lsp/keys.lua:285:   while index <= #lhs do
lua/magit-lsp/keys.lua:286:     local char = lhs:sub(index, index)
lua/magit-lsp/keys.lua:287:     if char == '<' then
lua/magit-lsp/keys.lua:288:       local close = lhs:find('>', index + 1, true)
lua/magit-lsp/keys.lua:289:       if not close then
lua/magit-lsp/keys.lua:290:         return nil, 'unterminated `<` in lhs'
lua/magit-lsp/keys.lua:291:       end
lua/magit-lsp/keys.lua:292:       local canonical, err = special_to_canonical(lhs:sub(index + 1, close - 1))
lua/magit-lsp/keys.lua:293:       if not canonical then
lua/magit-lsp/keys.lua:294:         return nil, err
lua/magit-lsp/keys.lua:295:       end
lua/magit-lsp/keys.lua:296:       steps[#steps + 1] = canonical
lua/magit-lsp/keys.lua:297:       index = close + 1
lua/magit-lsp/keys.lua:298:     elseif char == ' ' then
lua/magit-lsp/keys.lua:299:       steps[#steps + 1] = 'Space'
lua/magit-lsp/keys.lua:300:       index = index + 1
lua/magit-lsp/keys.lua:301:     elseif char == '+' then
lua/magit-lsp/keys.lua:302:       steps[#steps + 1] = 'Plus'
lua/magit-lsp/keys.lua:303:       index = index + 1
lua/magit-lsp/keys.lua:304:     else
lua/magit-lsp/keys.lua:305:       steps[#steps + 1] = char
lua/magit-lsp/keys.lua:306:       index = index + 1
lua/magit-lsp/keys.lua:307:     end
lua/magit-lsp/keys.lua:308:   end
lua/magit-lsp/keys.lua:309:
lua/magit-lsp/keys.lua:310:   return table.concat(steps, ' ')
lua/magit-lsp/keys.lua:311: end
lua/magit-lsp/keys.lua:312:
lua/magit-lsp/keys.lua:313: --- Termcodes for `nvim_feedkeys` with `escape_ks = false`.
lua/magit-lsp/keys.lua:314: ---@param lhs string
lua/magit-lsp/keys.lua:315: ---@return string
lua/magit-lsp/keys.lua:316: function M.termcodes(lhs)
lua/magit-lsp/keys.lua:317:   return vim.api.nvim_replace_termcodes(lhs, true, false, true)
lua/magit-lsp/keys.lua:318: end
lua/magit-lsp/keys.lua:319:
lua/magit-lsp/keys.lua:320: return M
```

VSCode typed-menu translation is intentionally narrower; generated static keybindings and native fold-key conversion are separate:

```js
clients/vscode/lib/menu.js:71: const SPECIAL_CHARS = {
clients/vscode/lib/menu.js:72:   ' ': 'Space',
clients/vscode/lib/menu.js:73:   '+': 'Plus',
clients/vscode/lib/menu.js:74: };
clients/vscode/lib/menu.js:75:
clients/vscode/lib/menu.js:76: /**
clients/vscode/lib/menu.js:77:  * The canonical key step for one typed character (§3.8).
clients/vscode/lib/menu.js:78:  *
clients/vscode/lib/menu.js:79:  * @param {string} character
clients/vscode/lib/menu.js:80:  * @returns {string}
clients/vscode/lib/menu.js:81:  */
clients/vscode/lib/menu.js:82: function charToKey(character) {
clients/vscode/lib/menu.js:83:   return SPECIAL_CHARS[character] || character;
clients/vscode/lib/menu.js:84: }
clients/vscode/lib/menu.js:85:
clients/vscode/lib/menu.js:86: /**
clients/vscode/lib/menu.js:87:  * Turn typed QuickPick input into `view/key` steps.
clients/vscode/lib/menu.js:88:  *
clients/vscode/lib/menu.js:89:  * Menu keys are characters (`c`) or two-step prefixes (`-a`, `=A`), so each
clients/vscode/lib/menu.js:90:  * typed character becomes its own key request and the server keeps the pending
clients/vscode/lib/menu.js:91:  * prefix. The QuickPick input is cleared after forwarding.
clients/vscode/lib/menu.js:92:  *
clients/vscode/lib/menu.js:93:  * @param {string|undefined} value
clients/vscode/lib/menu.js:94:  * @returns {string[]}
clients/vscode/lib/menu.js:95:  */
clients/vscode/lib/menu.js:96: function routeTyped(value) {
clients/vscode/lib/menu.js:97:   if (typeof value !== 'string') {
clients/vscode/lib/menu.js:98:     return [];
clients/vscode/lib/menu.js:99:   }
clients/vscode/lib/menu.js:100:   return Array.from(value).map(charToKey);
clients/vscode/lib/menu.js:101: }
clients/vscode/lib/menu.js:102:
clients/vscode/lib/menu.js:103: module.exports = {
clients/vscode/lib/menu.js:104:   quickPickItems,
clients/vscode/lib/menu.js:105:   quickPickItem,
clients/vscode/lib/menu.js:106:   describeItem,
clients/vscode/lib/menu.js:107:   routeTyped,
clients/vscode/lib/menu.js:108:   charToKey,
clients/vscode/lib/menu.js:109: };
clients/vscode/lib/foldkeys.js:24:  * The equivalent magit command for a VSCode fold command, if there is one.
clients/vscode/lib/foldkeys.js:25:  *
clients/vscode/lib/foldkeys.js:26:  * @param {string} command
clients/vscode/lib/foldkeys.js:27:  * @returns {{command: string, args?: object}|undefined}
clients/vscode/lib/foldkeys.js:28:  */
clients/vscode/lib/foldkeys.js:29: function mapFoldCommand(command) {
clients/vscode/lib/foldkeys.js:30:   if (typeof command !== 'string') {
clients/vscode/lib/foldkeys.js:31:     return undefined;
clients/vscode/lib/foldkeys.js:32:   }
clients/vscode/lib/foldkeys.js:33:   if (FOLD_COMMANDS[command]) {
clients/vscode/lib/foldkeys.js:34:     return FOLD_COMMANDS[command];
clients/vscode/lib/foldkeys.js:35:   }
clients/vscode/lib/foldkeys.js:36:   if (command === 'editor.toggleFold') {
clients/vscode/lib/foldkeys.js:37:     return { command: 'common.toggle-section' };
clients/vscode/lib/foldkeys.js:38:   }
clients/vscode/lib/foldkeys.js:39:   if (command.startsWith('editor.unfold')) {
clients/vscode/lib/foldkeys.js:40:     return { command: 'common.show-section' };
clients/vscode/lib/foldkeys.js:41:   }
clients/vscode/lib/foldkeys.js:42:   if (command.startsWith('editor.fold')) {
clients/vscode/lib/foldkeys.js:43:     return { command: 'common.hide-section' };
clients/vscode/lib/foldkeys.js:44:   }
clients/vscode/lib/foldkeys.js:45:   return undefined;
clients/vscode/lib/foldkeys.js:57: function mapFoldKeys(input) {
clients/vscode/lib/foldkeys.js:58:   const entries = typeof input === 'string' ? parseJsonc(input) : input;
clients/vscode/lib/foldkeys.js:59:   if (!Array.isArray(entries)) {
clients/vscode/lib/foldkeys.js:60:     return [];
clients/vscode/lib/foldkeys.js:61:   }
clients/vscode/lib/foldkeys.js:62:
clients/vscode/lib/foldkeys.js:63:   const mapped = [];
clients/vscode/lib/foldkeys.js:64:   for (const entry of entries) {
clients/vscode/lib/foldkeys.js:65:     if (!entry || typeof entry !== 'object') {
clients/vscode/lib/foldkeys.js:66:       continue;
clients/vscode/lib/foldkeys.js:67:     }
clients/vscode/lib/foldkeys.js:68:     const target = mapFoldCommand(entry.command);
clients/vscode/lib/foldkeys.js:69:     if (!target) {
clients/vscode/lib/foldkeys.js:70:       continue;
clients/vscode/lib/foldkeys.js:71:     }
clients/vscode/lib/foldkeys.js:72:
clients/vscode/lib/foldkeys.js:73:     const result = {
clients/vscode/lib/foldkeys.js:74:       key: entry.key,
clients/vscode/lib/foldkeys.js:75:       command: 'magit.invoke',
clients/vscode/lib/foldkeys.js:76:       args: { command: target.command },
clients/vscode/lib/foldkeys.js:77:       when: 'magit.active && !magit.textMode && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible',
clients/vscode/lib/foldkeys.js:78:     };
clients/vscode/lib/foldkeys.js:79:     if (target.args) {
clients/vscode/lib/foldkeys.js:80:       result.args.args = target.args;
clients/vscode/lib/foldkeys.js:81:     }
clients/vscode/lib/foldkeys.js:82:     // Keep platform-specific key overrides as VSCode's own schema expects.
clients/vscode/lib/foldkeys.js:83:     for (const platform of ['mac', 'linux', 'win']) {
clients/vscode/lib/foldkeys.js:84:       if (typeof entry[platform] === 'string') {
clients/vscode/lib/foldkeys.js:85:         result[platform] = entry[platform];
clients/vscode/lib/foldkeys.js:86:       }
clients/vscode/lib/foldkeys.js:87:     }
clients/vscode/lib/foldkeys.js:88:     mapped.push(result);
clients/vscode/lib/foldkeys.js:89:   }
clients/vscode/lib/foldkeys.js:90:   return mapped;
clients/vscode/lib/foldkeys.js:91: }
clients/vscode/lib/foldkeys.js:92:
clients/vscode/lib/foldkeys.js:93: const FOLD_COMMANDS = {
clients/vscode/lib/foldkeys.js:94:   'editor.fold': { command: 'common.hide-section' },
clients/vscode/lib/foldkeys.js:95:   'editor.foldRecursively': { command: 'common.hide-section', args: { recursive: true } },
clients/vscode/lib/foldkeys.js:96:   'editor.foldAll': { command: 'common.hide-all' },
clients/vscode/lib/foldkeys.js:97:   'editor.foldAllBlockComment': { command: 'common.hide-all' },
clients/vscode/lib/foldkeys.js:98:   'editor.foldAllMarkerRegions': { command: 'common.hide-all' },
clients/vscode/lib/foldkeys.js:99:   'editor.foldAllExcept': { command: 'common.hide-all' },
clients/vscode/lib/foldkeys.js:100:   'editor.unfold': { command: 'common.show-section' },
clients/vscode/lib/foldkeys.js:101:   'editor.unfoldRecursively': { command: 'common.show-section', args: { recursive: true } },
clients/vscode/lib/foldkeys.js:102:   'editor.unfoldAll': { command: 'common.show-all' },
clients/vscode/lib/foldkeys.js:103:   'editor.unfoldAllMarkerRegions': { command: 'common.show-all' },
clients/vscode/lib/foldkeys.js:104:   'editor.unfoldAllExcept': { command: 'common.show-all' },
clients/vscode/lib/foldkeys.js:105:   'editor.toggleFold': { command: 'common.toggle-section' },
clients/vscode/lib/foldkeys.js:106: };
clients/vscode/lib/foldkeys.js:107:
clients/vscode/lib/foldkeys.js:108: for (let level = 1; level <= 7; level += 1) {
clients/vscode/lib/foldkeys.js:109:   FOLD_COMMANDS[`editor.foldLevel${level}`] = { command: 'common.less-detail' };
clients/vscode/lib/foldkeys.js:110: }
clients/vscode/lib/foldkeys.js:111:
clients/vscode/lib/foldkeys.js:112: module.exports = {
clients/vscode/lib/foldkeys.js:113:   parseJsonc,
clients/vscode/lib/foldkeys.js:114:   mapFoldCommand,
clients/vscode/lib/foldkeys.js:115:   mapFoldKeys,
clients/vscode/lib/foldkeys.js:116: };
clients/vscode/scripts/gen-keybindings.mjs:32: const server = serverCommand();
clients/vscode/scripts/gen-keybindings.mjs:33: const output = execFileSync(
clients/vscode/scripts/gen-keybindings.mjs:34:   server.command,
clients/vscode/scripts/gen-keybindings.mjs:35:   [...server.args, 'keymap', '--format', 'vscode', '--profile', 'standard'],
clients/vscode/scripts/gen-keybindings.mjs:36:   { cwd: repoRoot, encoding: 'utf8' }
clients/vscode/scripts/gen-keybindings.mjs:37: );
clients/vscode/scripts/gen-keybindings.mjs:38:
clients/vscode/scripts/gen-keybindings.mjs:39: const keybindings = JSON.parse(output);
clients/vscode/scripts/gen-keybindings.mjs:40: const pkg = JSON.parse(readFileSync(manifestPath, 'utf8'));
clients/vscode/scripts/gen-keybindings.mjs:41: pkg.contributes = pkg.contributes || {};
clients/vscode/scripts/gen-keybindings.mjs:42:
clients/vscode/scripts/gen-keybindings.mjs:43: const current = JSON.stringify(pkg.contributes.keybindings || []);
clients/vscode/scripts/gen-keybindings.mjs:44: const next = JSON.stringify(keybindings);
clients/vscode/scripts/gen-keybindings.mjs:45:
clients/vscode/scripts/gen-keybindings.mjs:46: if (checkOnly) {
clients/vscode/scripts/gen-keybindings.mjs:47:   if (current !== next) {
clients/vscode/scripts/gen-keybindings.mjs:48:     console.error('contributes.keybindings is stale; run node clients/vscode/scripts/gen-keybindings.mjs');
clients/vscode/scripts/gen-keybindings.mjs:49:     process.exit(1);
clients/vscode/scripts/gen-keybindings.mjs:50:   }
clients/vscode/scripts/gen-keybindings.mjs:51:   console.log(`ok ${keybindings.length} keybindings`);
clients/vscode/scripts/gen-keybindings.mjs:52:   process.exit(0);
clients/vscode/scripts/gen-keybindings.mjs:53: }
clients/vscode/scripts/gen-keybindings.mjs:54:
clients/vscode/scripts/gen-keybindings.mjs:55: pkg.contributes.keybindings = keybindings;
clients/vscode/scripts/gen-keybindings.mjs:56: writeFileSync(manifestPath, `${JSON.stringify(pkg, null, 2)}\n`);
```

Comparison: Neovim implements canonical chord parse/render (`keys.lua`); VSCode's menu maps only typed space/plus and other characters, static bindings are generated from the server manifest, and fold keybindings preserve VSCode `entry.key`. The matching literal special names `Space`/`Plus` are identical where used; the translator tables/functions are not identical and do not have equivalent scope. No client-wide equivalence asserted.

### Token/theme maps

```lua
lua/magit-lsp/highlights.lua:10: --- Client-only overrides, token type to theme group.
lua/magit-lsp/highlights.lua:11: ---
lua/magit-lsp/highlights.lua:12: --- The server's suggestion wins for every type absent here, so this table is
lua/magit-lsp/highlights.lua:13: --- only for local preferences or a temporarily bad server suggestion.
lua/magit-lsp/highlights.lua:14: M.OVERRIDES = {}
lua/magit-lsp/highlights.lua:15:
lua/magit-lsp/highlights.lua:16: --- Modifier groups are applied above the type, so they should only add
lua/magit-lsp/highlights.lua:17: --- attributes (or a background) and never replace a type's foreground.
lua/magit-lsp/highlights.lua:18: M.MODIFIER_LINKS = {
lua/magit-lsp/highlights.lua:19:   -- keep-for: TASKS §3.15 recorded modifier styling; selected is used by log selection, remaining links support planned/ref presentation (R-5.3).
lua/magit-lsp/highlights.lua:20:   staged = 'Bold',
lua/magit-lsp/highlights.lua:21:   selected = 'Visual',
lua/magit-lsp/highlights.lua:22:   planned = 'Comment',
lua/magit-lsp/highlights.lua:23:   disabled = 'Comment',
lua/magit-lsp/highlights.lua:24:   bold = 'Bold',
lua/magit-lsp/highlights.lua:25: }
lua/magit-lsp/highlights.lua:26:
lua/magit-lsp/highlights.lua:27: --- The legend of the most recently initialized client, kept for `:colorscheme`.
lua/magit-lsp/highlights.lua:28: ---@type table|nil
lua/magit-lsp/highlights.lua:29: M.legend = nil
```

```js
clients/vscode/lib/semantic.js:1: 'use strict';
clients/vscode/lib/semantic.js:2:
clients/vscode/lib/semantic.js:3: /** Register theme-addressable tokens using the server legend and ranges verbatim. */
clients/vscode/lib/semantic.js:4: function register(vscode, client, stateForUri, subscriptions) {
clients/vscode/lib/semantic.js:5:   const serverLegend = client.initializeResult.capabilities.semanticTokensProvider.legend;
clients/vscode/lib/semantic.js:6:   const legend = new vscode.SemanticTokensLegend(
clients/vscode/lib/semantic.js:7:     serverLegend.tokenTypes.map((type) => type.replaceAll('.', '-')),
clients/vscode/lib/semantic.js:8:     serverLegend.tokenModifiers
clients/vscode/lib/semantic.js:9:   );
clients/vscode/lib/semantic.js:10:   const changed = new vscode.EventEmitter();
clients/vscode/lib/semantic.js:11:   subscriptions.push(changed, vscode.languages.registerDocumentSemanticTokensProvider(
clients/vscode/lib/semantic.js:12:     { scheme: 'magit', language: 'magit' },
clients/vscode/lib/semantic.js:13:     {
clients/vscode/lib/semantic.js:14:       onDidChangeSemanticTokens: changed.event,
clients/vscode/lib/semantic.js:15:       async provideDocumentSemanticTokens(document) {
clients/vscode/lib/semantic.js:16:         const state = stateForUri(document.uri);
clients/vscode/lib/semantic.js:17:         if (!state) return undefined;
clients/vscode/lib/semantic.js:18:         const tokens = await client.sendRequest('textDocument/semanticTokens/full', {
clients/vscode/lib/semantic.js:19:           textDocument: { uri: state.descriptor.uri },
clients/vscode/lib/semantic.js:20:         });
clients/vscode/lib/semantic.js:21:         return tokens && new vscode.SemanticTokens(new Uint32Array(tokens.data), tokens.resultId);
clients/vscode/lib/semantic.js:22:       },
clients/vscode/lib/semantic.js:23:     }, legend
clients/vscode/lib/semantic.js:24:   ));
clients/vscode/lib/semantic.js:25:   return changed;
clients/vscode/lib/semantic.js:26: }
clients/vscode/lib/semantic.js:27:
clients/vscode/lib/semantic.js:28: module.exports = { register };
clients/vscode/package.json:74:     "semanticTokenScopes": [
clients/vscode/package.json:75:       {
clients/vscode/package.json:76:         "language": "magit",
clients/vscode/package.json:77:         "scopes": {
clients/vscode/package.json:78:           "magit-section": [
clients/vscode/package.json:79:             "entity.name.section"
clients/vscode/package.json:80:           ],
clients/vscode/package.json:81:           "magit-count": [
clients/vscode/package.json:82:             "constant.numeric"
clients/vscode/package.json:83:           ],
clients/vscode/package.json:84:           "magit-file": [
clients/vscode/package.json:85:             "string"
clients/vscode/package.json:86:           ],
clients/vscode/package.json:87:           "magit-file-modified": [
clients/vscode/package.json:88:             "markup.changed"
clients/vscode/package.json:89:           ],
clients/vscode/package.json:90:           "magit-file-added": [
clients/vscode/package.json:91:             "markup.inserted"
clients/vscode/package.json:92:           ],
clients/vscode/package.json:93:           "magit-file-deleted": [
clients/vscode/package.json:94:             "markup.deleted"
clients/vscode/package.json:95:           ],
clients/vscode/package.json:96:           "magit-file-renamed": [
clients/vscode/package.json:97:             "markup.changed"
clients/vscode/package.json:98:           ],
clients/vscode/package.json:99:           "magit-file-untracked": [
clients/vscode/package.json:100:             "comment"
clients/vscode/package.json:101:           ],
clients/vscode/package.json:102:           "magit-hunk-header": [
clients/vscode/package.json:103:             "meta.diff.range"
clients/vscode/package.json:104:           ],
clients/vscode/package.json:105:           "magit-diff-added": [
clients/vscode/package.json:106:             "markup.inserted"
clients/vscode/package.json:107:           ],
clients/vscode/package.json:108:           "magit-diff-removed": [
clients/vscode/package.json:109:             "markup.deleted"
clients/vscode/package.json:110:           ],
clients/vscode/package.json:111:           "magit-message-error": [
clients/vscode/package.json:112:             "invalid"
clients/vscode/package.json:113:           ],
clients/vscode/package.json:114:           "magit-message-warn": [
clients/vscode/package.json:115:             "markup.warning"
clients/vscode/package.json:116:           ]
```

Neovim client-local modifier links are explicit; token colors derive from the server legend. VSCode transforms dotted token type identifiers to hyphenated identifiers and contributes theme scopes for the listed types; it passes modifiers through. These are structurally different; byte-for-byte equivalence is not claimed. The scope entries shown are the entire `semanticTokenScopes` mapping in the inspected range (with its literal JSON values).

### Prompt kinds/widgets and answers

```lua
lua/magit-lsp/prompt.lua:50: --- Run a text prompt; `on_value` receives the typed string.
lua/magit-lsp/prompt.lua:51: local function ask_text(client_id, params, on_value)
lua/magit-lsp/prompt.lua:52:   if params.secret then
lua/magit-lsp/prompt.lua:53:     -- `vim.ui.input` cannot mask, so secret answers use the built-in helper.
lua/magit-lsp/prompt.lua:54:     local value = vim.fn.inputsecret(params.title)
lua/magit-lsp/prompt.lua:55:     if value == nil or value == '' then
lua/magit-lsp/prompt.lua:56:       finish(client_id, params, true)
lua/magit-lsp/prompt.lua:57:     else
lua/magit-lsp/prompt.lua:58:       on_value(value)
lua/magit-lsp/prompt.lua:59:     end
lua/magit-lsp/prompt.lua:60:     return
lua/magit-lsp/prompt.lua:61:   end
lua/magit-lsp/prompt.lua:62:   local default = type(params.default) == 'string' and params.default or nil
lua/magit-lsp/prompt.lua:63:   vim.ui.input({ prompt = params.title, default = default }, function(input)
lua/magit-lsp/prompt.lua:64:     if input == nil then
lua/magit-lsp/prompt.lua:65:       finish(client_id, params, true)
lua/magit-lsp/prompt.lua:66:     else
lua/magit-lsp/prompt.lua:67:       on_value(input)
lua/magit-lsp/prompt.lua:68:     end
lua/magit-lsp/prompt.lua:69:   end)
lua/magit-lsp/prompt.lua:70: end
lua/magit-lsp/prompt.lua:71:
lua/magit-lsp/prompt.lua:72: local function ask_confirm(client_id, params)
lua/magit-lsp/prompt.lua:73:   local yes = params.yesLabel or 'Yes'
lua/magit-lsp/prompt.lua:74:   -- A destructive question focuses "No" first.
lua/magit-lsp/prompt.lua:75:   local no = params.noLabel or 'No'
lua/magit-lsp/prompt.lua:76:   local labels = (params.destructive or params.default == false) and { no, yes }
lua/magit-lsp/prompt.lua:77:     or { yes, no }
lua/magit-lsp/prompt.lua:78:   vim.ui.select(labels, { prompt = params.title }, function(choice)
lua/magit-lsp/prompt.lua:79:     if choice == nil then
lua/magit-lsp/prompt.lua:80:       finish(client_id, params, true)
lua/magit-lsp/prompt.lua:81:     else
lua/magit-lsp/prompt.lua:82:       finish(client_id, params, false, choice == yes)
lua/magit-lsp/prompt.lua:83:     end
lua/magit-lsp/prompt.lua:84:   end)
lua/magit-lsp/prompt.lua:85: end
lua/magit-lsp/prompt.lua:86:
lua/magit-lsp/prompt.lua:87: local function ask_choice(client_id, params)
lua/magit-lsp/prompt.lua:88:   local items = {}
lua/magit-lsp/prompt.lua:89:   for _, item in ipairs(params.items or {}) do
lua/magit-lsp/prompt.lua:90:     items[#items + 1] = item
lua/magit-lsp/prompt.lua:91:   end
lua/magit-lsp/prompt.lua:92:   if params.allowCustom then
lua/magit-lsp/prompt.lua:93:     items[#items + 1] = { id = '__custom__', label = 'Enter custom value…' }
lua/magit-lsp/prompt.lua:94:   end
lua/magit-lsp/prompt.lua:95:   vim.ui.select(items, {
lua/magit-lsp/prompt.lua:96:     prompt = params.title,
lua/magit-lsp/prompt.lua:97:     format_item = format_item,
lua/magit-lsp/prompt.lua:98:   }, function(item)
lua/magit-lsp/prompt.lua:99:     if item == nil then
lua/magit-lsp/prompt.lua:100:       finish(client_id, params, true)
lua/magit-lsp/prompt.lua:101:     elseif item.id == '__custom__' then
lua/magit-lsp/prompt.lua:102:       ask_text(client_id, params, function(text)
lua/magit-lsp/prompt.lua:103:         finish(client_id, params, false, { custom = text })
lua/magit-lsp/prompt.lua:104:       end)
lua/magit-lsp/prompt.lua:105:     else
lua/magit-lsp/prompt.lua:106:       finish(client_id, params, false, { id = item.id })
lua/magit-lsp/prompt.lua:107:     end
lua/magit-lsp/prompt.lua:108:   end)
lua/magit-lsp/prompt.lua:109: end
lua/magit-lsp/prompt.lua:110:
lua/magit-lsp/prompt.lua:111: local function ask_multi(client_id, params)
lua/magit-lsp/prompt.lua:112:   local selected = {}
lua/magit-lsp/prompt.lua:113:   local order = {}
lua/magit-lsp/prompt.lua:114:   for _, id in ipairs(params.preselected or {}) do
lua/magit-lsp/prompt.lua:115:     if not selected[id] then
lua/magit-lsp/prompt.lua:116:       selected[id] = true
lua/magit-lsp/prompt.lua:117:       order[#order + 1] = id
lua/magit-lsp/prompt.lua:118:     end
lua/magit-lsp/prompt.lua:119:   end
lua/magit-lsp/prompt.lua:120:
lua/magit-lsp/prompt.lua:121:   local function step()
lua/magit-lsp/prompt.lua:122:     local items = {}
lua/magit-lsp/prompt.lua:123:     for _, item in ipairs(params.items or {}) do
lua/magit-lsp/prompt.lua:124:       items[#items + 1] = {
lua/magit-lsp/prompt.lua:125:         id = item.id,
lua/magit-lsp/prompt.lua:126:         label = (selected[item.id] and '[x] ' or '[ ] ') .. item.label,
lua/magit-lsp/prompt.lua:127:         description = item.description,
lua/magit-lsp/prompt.lua:128:         detail = item.detail,
lua/magit-lsp/prompt.lua:129:       }
lua/magit-lsp/prompt.lua:130:     end
lua/magit-lsp/prompt.lua:131:     items[#items + 1] = { id = '__done__', label = 'Done' }
lua/magit-lsp/prompt.lua:132:
lua/magit-lsp/prompt.lua:133:     vim.ui.select(items, {
lua/magit-lsp/prompt.lua:134:       prompt = params.title,
lua/magit-lsp/prompt.lua:135:       format_item = format_item,
lua/magit-lsp/prompt.lua:136:     }, function(item)
lua/magit-lsp/prompt.lua:137:       if item == nil then
lua/magit-lsp/prompt.lua:138:         finish(client_id, params, true)
lua/magit-lsp/prompt.lua:139:       elseif item.id == '__done__' then
lua/magit-lsp/prompt.lua:140:         finish(client_id, params, false, { ids = order })
lua/magit-lsp/prompt.lua:141:       else
lua/magit-lsp/prompt.lua:142:         if selected[item.id] then
lua/magit-lsp/prompt.lua:143:           selected[item.id] = nil
lua/magit-lsp/prompt.lua:144:           for index, id in ipairs(order) do
lua/magit-lsp/prompt.lua:145:             if id == item.id then
lua/magit-lsp/prompt.lua:146:               table.remove(order, index)
lua/magit-lsp/prompt.lua:147:               break
lua/magit-lsp/prompt.lua:148:             end
lua/magit-lsp/prompt.lua:149:           end
lua/magit-lsp/prompt.lua:150:         else
lua/magit-lsp/prompt.lua:151:           selected[item.id] = true
lua/magit-lsp/prompt.lua:152:           order[#order + 1] = item.id
lua/magit-lsp/prompt.lua:153:         end
lua/magit-lsp/prompt.lua:154:         step()
lua/magit-lsp/prompt.lua:155:       end
lua/magit-lsp/prompt.lua:156:     end)
lua/magit-lsp/prompt.lua:157:   end
lua/magit-lsp/prompt.lua:158:
lua/magit-lsp/prompt.lua:159:   step()
lua/magit-lsp/prompt.lua:160: end
lua/magit-lsp/prompt.lua:161:
lua/magit-lsp/prompt.lua:162: --- Show one `view/prompt` notification and answer it.
lua/magit-lsp/prompt.lua:163: ---@param params table `view/prompt` params
lua/magit-lsp/prompt.lua:164: ---@param client_id integer|nil originating LSP client
lua/magit-lsp/prompt.lua:165: function M.handle(params, client_id)
lua/magit-lsp/prompt.lua:166:   if type(params) ~= 'table' or not params.promptId then
lua/magit-lsp/prompt.lua:167:     return
lua/magit-lsp/prompt.lua:168:   end
lua/magit-lsp/prompt.lua:169:   hooks.prompt_started()
lua/magit-lsp/prompt.lua:170:   local kind = params.kind
lua/magit-lsp/prompt.lua:171:   if kind == 'confirm' then
lua/magit-lsp/prompt.lua:172:     ask_confirm(client_id, params)
lua/magit-lsp/prompt.lua:173:   elseif kind == 'text' then
lua/magit-lsp/prompt.lua:174:     ask_text(client_id, params, function(text)
lua/magit-lsp/prompt.lua:175:       finish(client_id, params, false, text)
lua/magit-lsp/prompt.lua:176:     end)
lua/magit-lsp/prompt.lua:177:   elseif kind == 'choice' then
lua/magit-lsp/prompt.lua:178:     ask_choice(client_id, params)
lua/magit-lsp/prompt.lua:179:   elseif kind == 'multiChoice' then
lua/magit-lsp/prompt.lua:180:     ask_multi(client_id, params)
lua/magit-lsp/prompt.lua:181:   else
lua/magit-lsp/prompt.lua:182:     -- An unknown kind cannot be repaired client-side; cancel it.
lua/magit-lsp/prompt.lua:183:     finish(client_id, params, true)
lua/magit-lsp/prompt.lua:184:   end
lua/magit-lsp/prompt.lua:185: end
```

```js
clients/vscode/lib/prompt.js:1: 'use strict';
clients/vscode/lib/prompt.js:2:
clients/vscode/lib/prompt.js:3: /**
clients/vscode/lib/prompt.js:4:  * Pure mapping between `view/prompt` parameters and QuickPick items/answers.
clients/vscode/lib/prompt.js:5:  *
clients/vscode/lib/prompt.js:6:  * The extension owns the actual VSCode widgets; this module only shapes data
clients/vscode/lib/prompt.js:7:  * so the mapping can be tested without an editor.
clients/vscode/lib/prompt.js:8:  */
clients/vscode/lib/prompt.js:9:
clients/vscode/lib/prompt.js:10: /** The id of the synthetic "type a value" row of an `allowCustom` choice. */
clients/vscode/lib/prompt.js:11: const CUSTOM_ID = '__custom__';
clients/vscode/lib/prompt.js:12:
clients/vscode/lib/prompt.js:13: /**
clients/vscode/lib/prompt.js:14:  * The rows a prompt offers, in display order.
clients/vscode/lib/prompt.js:15:  *
clients/vscode/lib/prompt.js:16:  * @param {object} params `view/prompt` params
clients/vscode/lib/prompt.js:17:  * @returns {{id: string, label: string, description?: string, detail?: string}[]}
clients/vscode/lib/prompt.js:18:  */
clients/vscode/lib/prompt.js:19: function promptItems(params) {
clients/vscode/lib/prompt.js:20:   if (!params) {
clients/vscode/lib/prompt.js:21:     return [];
clients/vscode/lib/prompt.js:22:   }
clients/vscode/lib/prompt.js:23:   if (params.kind === 'confirm') {
clients/vscode/lib/prompt.js:24:     const yes = { id: 'yes', label: params.yesLabel || 'Yes' };
clients/vscode/lib/prompt.js:25:     const no = { id: 'no', label: params.noLabel || 'No' };
clients/vscode/lib/prompt.js:26:     // A destructive question focuses "No" first.
clients/vscode/lib/prompt.js:27:     return (params.destructive || params.default === false) ? [no, yes] : [yes, no];
clients/vscode/lib/prompt.js:28:   }
clients/vscode/lib/prompt.js:29:
clients/vscode/lib/prompt.js:30:   const items = (params.items || []).map((item) => ({
clients/vscode/lib/prompt.js:31:     id: item.id,
clients/vscode/lib/prompt.js:32:     label: item.label,
clients/vscode/lib/prompt.js:33:     description: item.description,
clients/vscode/lib/prompt.js:34:     detail: item.detail,
clients/vscode/lib/prompt.js:35:   }));
clients/vscode/lib/prompt.js:36:   if (params.kind === 'choice' && params.allowCustom) {
clients/vscode/lib/prompt.js:37:     items.push({ id: CUSTOM_ID, label: 'Enter custom value…' });
clients/vscode/lib/prompt.js:38:   }
clients/vscode/lib/prompt.js:39:   return items;
clients/vscode/lib/prompt.js:40: }
clients/vscode/lib/prompt.js:41:
clients/vscode/lib/prompt.js:42: /**
clients/vscode/lib/prompt.js:43:  * Shape the answer for one picked row (or value).
clients/vscode/lib/prompt.js:44:  *
clients/vscode/lib/prompt.js:45:  * @param {object} params `view/prompt` params
clients/vscode/lib/prompt.js:46:  * @param {unknown} picked one row, a string, an array of rows, or undefined
clients/vscode/lib/prompt.js:47:  * @returns {{cancelled: boolean, value?: unknown, custom?: boolean}}
clients/vscode/lib/prompt.js:48:  */
clients/vscode/lib/prompt.js:49: function answerFor(params, picked) {
clients/vscode/lib/prompt.js:50:   if (picked === undefined || picked === null) {
clients/vscode/lib/prompt.js:51:     return { cancelled: true };
clients/vscode/lib/prompt.js:52:   }
clients/vscode/lib/prompt.js:53:   switch (params.kind) {
clients/vscode/lib/prompt.js:54:     case 'confirm':
clients/vscode/lib/prompt.js:55:       return { cancelled: false, value: picked.id === 'yes' };
clients/vscode/lib/prompt.js:56:     case 'text':
clients/vscode/lib/prompt.js:57:       return { cancelled: false, value: String(picked) };
clients/vscode/lib/prompt.js:58:     case 'choice':
clients/vscode/lib/prompt.js:59:       if (picked.id === CUSTOM_ID) {
clients/vscode/lib/prompt.js:60:         // The caller must run a text prompt and use `customChoice`.
clients/vscode/lib/prompt.js:61:         return { cancelled: false, custom: true };
clients/vscode/lib/prompt.js:62:       }
clients/vscode/lib/prompt.js:63:       return { cancelled: false, value: { id: picked.id } };
clients/vscode/lib/prompt.js:64:     case 'multiChoice': {
clients/vscode/lib/prompt.js:65:       const rows = Array.isArray(picked) ? picked : [picked];
clients/vscode/lib/prompt.js:66:       return { cancelled: false, value: { ids: rows.map((row) => row.id) } };
clients/vscode/lib/prompt.js:67:     }
clients/vscode/lib/prompt.js:68:     default:
clients/vscode/lib/prompt.js:69:       return { cancelled: true };
clients/vscode/lib/prompt.js:70:   }
clients/vscode/lib/prompt.js:71: }
clients/vscode/lib/prompt.js:72:
clients/vscode/lib/prompt.js:73: /**
clients/vscode/lib/prompt.js:74:  * The answer value for a custom choice after the text prompt.
clients/vscode/lib/prompt.js:75:  *
clients/vscode/lib/prompt.js:76:  * @param {string|undefined} text the typed value, or undefined on cancel
clients/vscode/lib/prompt.js:77:  * @returns {{cancelled: boolean, value?: {custom: string}}}
clients/vscode/lib/prompt.js:78:  */
clients/vscode/lib/prompt.js:79: function customChoice(text) {
clients/vscode/lib/prompt.js:80:   if (text === undefined || text === null) {
clients/vscode/lib/prompt.js:81:     return { cancelled: true };
clients/vscode/lib/prompt.js:82:   }
clients/vscode/lib/prompt.js:83:   return { cancelled: false, value: { custom: text } };
clients/vscode/lib/prompt.js:84: }
clients/vscode/lib/prompt.js:85:
clients/vscode/lib/prompt.js:86: module.exports = { CUSTOM_ID, promptItems, answerFor, customChoice };
clients/vscode/lib/requests.js:183: async function promptFor(params) {
clients/vscode/lib/requests.js:184:   if (params.kind === 'text') {
clients/vscode/lib/requests.js:185:     const value = await vscode.window.showInputBox({
clients/vscode/lib/requests.js:186:       title: params.title,
clients/vscode/lib/requests.js:187:       prompt: params.detail,
clients/vscode/lib/requests.js:188:       placeHolder: params.placeholder,
clients/vscode/lib/requests.js:189:       value: typeof params.default === 'string' ? params.default : undefined,
clients/vscode/lib/requests.js:190:       password: Boolean(params.secret),
clients/vscode/lib/requests.js:191:       ignoreFocusOut: true,
clients/vscode/lib/requests.js:192:     });
clients/vscode/lib/requests.js:193:     return prompt.answerFor(params, value);
clients/vscode/lib/requests.js:194:   }
clients/vscode/lib/requests.js:195:
clients/vscode/lib/requests.js:196:   if (params.kind === 'confirm' || params.kind === 'choice') {
clients/vscode/lib/requests.js:197:     const picked = await vscode.window.showQuickPick(prompt.promptItems(params), {
clients/vscode/lib/requests.js:198:       title: params.title,
clients/vscode/lib/requests.js:199:       placeHolder: params.detail || params.title,
clients/vscode/lib/requests.js:200:       ignoreFocusOut: true,
clients/vscode/lib/requests.js:201:     });
clients/vscode/lib/requests.js:202:     const answer = prompt.answerFor(params, picked);
clients/vscode/lib/requests.js:203:     if (!answer.custom) {
clients/vscode/lib/requests.js:204:       return answer;
clients/vscode/lib/requests.js:205:     }
clients/vscode/lib/requests.js:206:     const text = await vscode.window.showInputBox({
clients/vscode/lib/requests.js:207:       title: params.title,
clients/vscode/lib/requests.js:208:       placeHolder: params.placeholder || 'Value',
clients/vscode/lib/requests.js:209:       ignoreFocusOut: true,
clients/vscode/lib/requests.js:210:     });
clients/vscode/lib/requests.js:211:     return prompt.customChoice(text);
clients/vscode/lib/requests.js:212:   }
clients/vscode/lib/requests.js:213:
clients/vscode/lib/requests.js:214:   if (params.kind === 'multiChoice') {
clients/vscode/lib/requests.js:215:     const preselected = new Set(params.preselected || []);
clients/vscode/lib/requests.js:216:     const items = prompt.promptItems(params).map((row) => ({
clients/vscode/lib/requests.js:217:       ...row,
clients/vscode/lib/requests.js:218:       picked: preselected.has(row.id),
clients/vscode/lib/requests.js:219:     }));
clients/vscode/lib/requests.js:220:     const picked = await vscode.window.showQuickPick(items, {
clients/vscode/lib/requests.js:221:       title: params.title,
clients/vscode/lib/requests.js:222:       placeHolder: params.detail || params.title,
clients/vscode/lib/requests.js:223:       canPickMany: true,
clients/vscode/lib/requests.js:224:       ignoreFocusOut: true,
clients/vscode/lib/requests.js:225:     });
clients/vscode/lib/requests.js:226:     return prompt.answerFor(params, picked);
clients/vscode/lib/requests.js:227:   }
clients/vscode/lib/requests.js:228:
clients/vscode/lib/requests.js:229:   // An unknown kind cannot be repaired client-side; cancel it.
clients/vscode/lib/requests.js:230:   return { cancelled: true };
clients/vscode/lib/requests.js:231: }
```

Both recognize confirm/text/choice/multiChoice and cancel unknown kinds; widget code and answer flows differ (Neovim `vim.ui`/`inputsecret`; VSCode InputBox/QuickPick). This is semantic correspondence, not identical tables.

### Effect and placement names/branches

```lua
lua/magit-lsp/config.lua:9: --- The status placements this client implements (R-UI-7).
lua/magit-lsp/config.lua:10: M.PLACEMENTS = { 'replace', 'vsplit', 'split', 'tab' }
lua/magit-lsp/config.lua:11:
lua/magit-lsp/config.lua:97: function M.client_capabilities()
lua/magit-lsp/config.lua:98:   local capabilities = vim.lsp.protocol.make_client_capabilities()
lua/magit-lsp/config.lua:99:   capabilities.experimental = capabilities.experimental or {}
lua/magit-lsp/config.lua:100:   capabilities.experimental.magit = {
lua/magit-lsp/config.lua:101:     v = 1,
lua/magit-lsp/config.lua:102:     keys = 'dynamic',
lua/magit-lsp/config.lua:103:     modifiers = M.key_modifiers(),
lua/magit-lsp/config.lua:104:     render = 'edits',
lua/magit-lsp/config.lua:105:     layout = { 'main', 'side', 'bottom', 'float' },
lua/magit-lsp/config.lua:106:     effects = { 'visitFile', 'editDocument', 'clipboard' },
lua/magit-lsp/config.lua:107:     prompts = { 'confirm', 'text', 'choice', 'multiChoice', 'secret' },
lua/magit-lsp/config.lua:108:     folds = 'server',
lua/magit-lsp/config.lua:109:   }
lua/magit-lsp/config.lua:110:   capabilities.prompts = true
lua/magit-lsp/config.lua:111:   capabilities.placements = { 'main', 'side', 'bottom', 'float' }
lua/magit-lsp/config.lua:112:   capabilities.editorBridge = false
lua/magit-lsp/config.lua:113:   return capabilities
lua/magit-lsp/init.lua:264:   -- implementations, not server descriptor placements.
lua/magit-lsp/init.lua:265:   view.register_placement('replace', { place = status_placement(nil) })
lua/magit-lsp/init.lua:266:   view.register_placement('vsplit', { place = status_placement('vsplit') })
lua/magit-lsp/init.lua:267:   view.register_placement('split', { place = status_placement('split') })
lua/magit-lsp/init.lua:268:   view.register_placement('tab', { place = status_placement('tabnew') })
lua/magit-lsp/effects.lua:14: local LEVELS = {
lua/magit-lsp/effects.lua:15:   info = vim.log.levels.INFO,
lua/magit-lsp/effects.lua:16:   warn = vim.log.levels.WARN,
lua/magit-lsp/effects.lua:17:   error = vim.log.levels.ERROR,
lua/magit-lsp/effects.lua:18: }
lua/magit-lsp/effects.lua:25: function M.notify(effect)
lua/magit-lsp/effects.lua:26:   M.last_error_detail = effect.detail
lua/magit-lsp/effects.lua:27:   local text = effect.text or ''
lua/magit-lsp/effects.lua:28:   if effect.hint then
lua/magit-lsp/effects.lua:29:     text = text .. ' - ' .. effect.hint
lua/magit-lsp/effects.lua:30:   end
lua/magit-lsp/effects.lua:31:   text = text:gsub('%s*\n%s*', ' ')
lua/magit-lsp/effects.lua:32:   -- A notification wider than the screen wraps and opens the hit-enter prompt,
lua/magit-lsp/effects.lua:33:   -- so keep a margin below `columns`.
lua/magit-lsp/effects.lua:34:   local limit = math.max(24, vim.o.columns - 12)
lua/magit-lsp/effects.lua:35:   if #text > limit then
lua/magit-lsp/effects.lua:36:     text = text:sub(1, limit - 3) .. '...'
lua/magit-lsp/effects.lua:37:   end
lua/magit-lsp/effects.lua:38:   vim.notify(text, LEVELS[effect.level] or vim.log.levels.INFO)
lua/magit-lsp/effects.lua:39: end
lua/magit-lsp/effects.lua:46: function M.copy(text)
lua/magit-lsp/effects.lua:47:   local ok = pcall(vim.fn.setreg, '+', text)
lua/magit-lsp/effects.lua:48:   pcall(vim.fn.setreg, '"', text)
lua/magit-lsp/effects.lua:49:   local preview = text:gsub('\n', ' ⏎ ')
lua/magit-lsp/effects.lua:50:   if #preview > 60 then
lua/magit-lsp/effects.lua:51:     preview = preview:sub(1, 57) .. '...'
lua/magit-lsp/effects.lua:52:   end
lua/magit-lsp/effects.lua:53:   if ok then
lua/magit-lsp/effects.lua:54:     vim.notify('Copied: ' .. preview, vim.log.levels.INFO)
lua/magit-lsp/effects.lua:55:   else
lua/magit-lsp/effects.lua:56:     vim.notify('Copied: ' .. preview .. ' (clipboard unavailable)', vim.log.levels.WARN)
lua/magit-lsp/effects.lua:57:   end
lua/magit-lsp/effects.lua:58: end
lua/magit-lsp/effects.lua:59:
lua/magit-lsp/effects.lua:60: --- Replay a server `passThrough` key natively, without re-triggering our maps.
lua/magit-lsp/effects.lua:61: ---@param key string canonical chord, e.g. `Escape` or `g Q`
lua/magit-lsp/effects.lua:62: function M.replay(key)
lua/magit-lsp/effects.lua:63:   local lhs = keys.to_lhs(key)
lua/magit-lsp/effects.lua:64:   if not lhs then
lua/magit-lsp/effects.lua:65:     return
lua/magit-lsp/effects.lua:66:   end
lua/magit-lsp/effects.lua:67:   -- `n` means "do not remap", so the client's own buffer-local map is skipped.
lua/magit-lsp/effects.lua:68:   vim.api.nvim_feedkeys(keys.termcodes(lhs), 'n', false)
lua/magit-lsp/effects.lua:101: function M.visit_file(effect)
lua/magit-lsp/effects.lua:102:   if type(effect) ~= 'table' or not effect.uri then
lua/magit-lsp/effects.lua:103:     return
lua/magit-lsp/effects.lua:104:   end
lua/magit-lsp/effects.lua:105:   local filename = vim.uri_to_fname(effect.uri)
lua/magit-lsp/effects.lua:106:   local target = visit_window()
lua/magit-lsp/effects.lua:107:   if vim.api.nvim_get_current_win() ~= target then
lua/magit-lsp/effects.lua:108:     vim.api.nvim_set_current_win(target)
lua/magit-lsp/effects.lua:109:   end
lua/magit-lsp/effects.lua:110:   vim.cmd('edit ' .. vim.fn.fnameescape(filename))
lua/magit-lsp/effects.lua:111:   local line = math.max((effect.line or 0) + 1, 1)
lua/magit-lsp/effects.lua:112:   local column = math.max(effect.column or 0, 0)
lua/magit-lsp/effects.lua:113:   vim.api.nvim_win_set_cursor(0, { line, column })
lua/magit-lsp/effects.lua:114: end
lua/magit-lsp/effects.lua:115:
```

```js
clients/vscode/lib/session.js:41:         initialize(params, second, third) {
clients/vscode/lib/session.js:42:           params.clientInfo = { name: 'Visual Studio Code', version: vscode.version };
clients/vscode/lib/session.js:43:           params.capabilities = params.capabilities || {};
clients/vscode/lib/session.js:44:           params.capabilities.experimental = params.capabilities.experimental || {};
clients/vscode/lib/session.js:45:           params.capabilities.experimental.magit = {
clients/vscode/lib/session.js:46:             v: 1,
clients/vscode/lib/session.js:47:             keys: 'static',
clients/vscode/lib/session.js:48:             modifiers: 'full',
clients/vscode/lib/session.js:49:             render: 'edits',
clients/vscode/lib/session.js:50:             layout: ['main', 'side', 'bottom', 'float'],
clients/vscode/lib/session.js:51:             effects: ['visitFile', 'editDocument', 'clipboard'],
clients/vscode/lib/session.js:52:             prompts: ['confirm', 'text', 'choice', 'multiChoice', 'secret'],
clients/vscode/lib/session.js:53:             folds: 'server',
clients/vscode/lib/session.js:54:           };
clients/vscode/lib/session.js:55:           const next = typeof second === 'function' ? second : third;
clients/vscode/lib/session.js:56:           return next(params);
clients/vscode/lib/session.js:57:         },
clients/vscode/lib/effects.js:9: const REPLAY_COMMANDS = {
clients/vscode/lib/effects.js:10:   'ctrl+c': 'editor.action.clipboardCopyAction',
clients/vscode/lib/effects.js:11:   'ctrl+x': 'editor.action.clipboardCutAction',
clients/vscode/lib/effects.js:12:   'ctrl+v': 'editor.action.clipboardPasteAction',
clients/vscode/lib/effects.js:13: };
clients/vscode/lib/effects.js:14:
clients/vscode/lib/effects.js:15: /**
clients/vscode/lib/effects.js:16:  * Perform an editor behaviour the server passed through.
clients/vscode/lib/effects.js:17:  *
clients/vscode/lib/effects.js:18:  * A static client cannot translate arbitrary chords into VSCode commands, so
clients/vscode/lib/effects.js:19:  * printable characters and clipboard shortcuts are the supported cases.
clients/vscode/lib/effects.js:20:  *
clients/vscode/lib/effects.js:21:  * @param {string} key
clients/vscode/lib/effects.js:22:  */
clients/vscode/lib/effects.js:23: async function replay(key) {
clients/vscode/lib/effects.js:24:   if (REPLAY_COMMANDS[key]) {
clients/vscode/lib/effects.js:25:     await vscode.commands.executeCommand(REPLAY_COMMANDS[key]);
clients/vscode/lib/effects.js:26:     return;
clients/vscode/lib/effects.js:27:   }
clients/vscode/lib/effects.js:28:   if (key === 'Space') {
clients/vscode/lib/effects.js:29:     await vscode.commands.executeCommand('default:type', { text: ' ' });
clients/vscode/lib/effects.js:30:     return;
clients/vscode/lib/effects.js:31:   }
clients/vscode/lib/effects.js:32:   if (typeof key === 'string' && Array.from(key).length === 1) {
clients/vscode/lib/effects.js:33:     await vscode.commands.executeCommand('default:type', { text: key });
clients/vscode/lib/effects.js:34:     return;
clients/vscode/lib/effects.js:35:   }
clients/vscode/lib/effects.js:36:   store.output.appendLine(`passThrough: cannot replay ${key} from a static binding`);
clients/vscode/lib/effects.js:37: }
clients/vscode/lib/effects.js:44: function showMessage(effect) {
clients/vscode/lib/effects.js:45:   const text = effect.hint ? `${effect.text}\n${effect.hint}` : effect.text || '';
clients/vscode/lib/effects.js:46:   if (effect.detail) {
clients/vscode/lib/effects.js:47:     store.output.appendLine(effect.detail);
clients/vscode/lib/effects.js:48:   }
clients/vscode/lib/effects.js:49:   if (effect.level === 'error') {
clients/vscode/lib/effects.js:50:     vscode.window.showErrorMessage(text);
clients/vscode/lib/effects.js:51:   } else if (effect.level === 'warn') {
clients/vscode/lib/effects.js:52:     vscode.window.showWarningMessage(text);
clients/vscode/lib/effects.js:53:   } else {
clients/vscode/lib/effects.js:54:     vscode.window.showInformationMessage(text);
clients/vscode/lib/effects.js:55:   }
clients/vscode/lib/effects.js:56: }
clients/vscode/lib/effects.js:63: async function applyEffects(effectList, originViewId) {
clients/vscode/lib/effects.js:64:   const origin = originViewId && store.views.get(originViewId);
clients/vscode/lib/effects.js:65:   let rootId = origin && origin.rootId;
clients/vscode/lib/effects.js:66:   for (const effect of effectList || []) {
clients/vscode/lib/effects.js:67:     switch (effect.type) {
clients/vscode/lib/effects.js:68:       case 'message':
clients/vscode/lib/effects.js:69:         showMessage(effect);
clients/vscode/lib/effects.js:70:         break;
clients/vscode/lib/effects.js:71:       case 'clipboard':
clients/vscode/lib/effects.js:72:         await vscode.env.clipboard.writeText(effect.text || '');
clients/vscode/lib/effects.js:73:         vscode.window.showInformationMessage('Copied to clipboard.');
clients/vscode/lib/effects.js:74:         break;
clients/vscode/lib/effects.js:75:       case 'passThrough':
clients/vscode/lib/effects.js:76:         await replay(effect.key);
clients/vscode/lib/effects.js:77:         break;
clients/vscode/lib/effects.js:78:       case 'openView':
clients/vscode/lib/effects.js:79:       case 'pushView':
clients/vscode/lib/effects.js:80:         {
clients/vscode/lib/effects.js:81:           const state = views.ensureView(effect.descriptor);
clients/vscode/lib/effects.js:82:           if (state) state.rootId = rootId || (effect.type === 'openView' ? state.id : undefined);
clients/vscode/lib/effects.js:83:         }
clients/vscode/lib/effects.js:84:         await views.showView(effect.descriptor);
clients/vscode/lib/effects.js:85:         break;
clients/vscode/lib/effects.js:86:       case 'popView':
clients/vscode/lib/effects.js:87:         await views.closeView(effect.viewId, true);
clients/vscode/lib/effects.js:88:         break;
clients/vscode/lib/effects.js:89:       case 'focusView':
clients/vscode/lib/effects.js:90:         await views.focusView(effect.viewId);
clients/vscode/lib/effects.js:91:         break;
clients/vscode/lib/effects.js:92:       case 'quit':
clients/vscode/lib/effects.js:93:         await views.closeRoot(rootId || originViewId);
clients/vscode/lib/effects.js:94:         break;
clients/vscode/lib/effects.js:95:       case 'visitFile': {
clients/vscode/lib/effects.js:96:         const document = await vscode.workspace.openTextDocument(
clients/vscode/lib/effects.js:97:           vscode.Uri.parse(effect.uri)
clients/vscode/lib/effects.js:98:         );
clients/vscode/lib/effects.js:99:         const editor = await vscode.window.showTextDocument(document, {
clients/vscode/lib/effects.js:100:           preview: false,
clients/vscode/lib/effects.js:101:           preserveFocus: false,
clients/vscode/lib/effects.js:102:         });
clients/vscode/lib/effects.js:103:         const position = new vscode.Position(effect.line || 0, effect.column || 0);
clients/vscode/lib/effects.js:104:         editor.selection = new vscode.Selection(position, position);
clients/vscode/lib/effects.js:105:         editor.revealRange(
clients/vscode/lib/effects.js:106:           new vscode.Range(position, position),
clients/vscode/lib/effects.js:107:           vscode.TextEditorRevealType.InCenterIfOutsideViewport
clients/vscode/lib/effects.js:108:         );
clients/vscode/lib/effects.js:109:         break;
clients/vscode/lib/effects.js:110:       }
clients/vscode/lib/effects.js:111:       case 'editDocument':
clients/vscode/lib/effects.js:112:         await drafts.open(effect, rootId || originViewId);
clients/vscode/lib/effects.js:113:         break;
clients/vscode/lib/effects.js:114:       default:
clients/vscode/lib/effects.js:115:         store.output.appendLine(`unknown effect: ${effect.type}`);
clients/vscode/lib/effects.js:116:     }
clients/vscode/lib/effects.js:117:   }
clients/vscode/lib/effects.js:118: }
clients/vscode/lib/effects.js:119:
clients/vscode/lib/effects.js:120: exports.applyEffects = applyEffects;
clients/vscode/lib/effects.js:121: exports.showMessage = showMessage;
clients/vscode/lib/effects.js:122: exports.replay = replay;
```

The advertised effect and prompt array string values in Lua `config.lua` and VSCode `session.js` are byte-identical literal sequences: effects `visitFile, editDocument, clipboard`; prompts `confirm, text, choice, multiChoice, secret`. This does not imply equal implementations. Neovim has status placements `replace/vsplit/split/tab`; no corresponding VSCode placement table appears in inspected package contributions or virtual-document view code. Layout capability values (`main/side/bottom/float`) match but are not status placement values. VSCode effect cases and Neovim routing are shown verbatim above.

## E. Initialize traces

Extraction/search commands: `find build -type f \( -name '*.jsonl' -o -name '*.log' \) | sort`; `grep -n -E 'initialize|didChange|clientInfo|capabilities' build/helix_lsp_probe.jsonl build/test_helix_lsp.jsonl build/test-output/trace-lua.jsonl build/ui/plain-render/with-final-newline/state/nvim/lsp.log build/ui/plain-render/without-final-newline/state/nvim/lsp.log`; `python3` JSONL extraction of Helix `data.params`; and `Path('build/ui/plain-render/with-final-newline/state/nvim/lsp.log').read_text().splitlines()[2]` for the full Neovim `init_params` literal. The latter is inserted verbatim below, not converted to JSON.

### Helix: actual captured JSONL

`build/helix_lsp_probe.jsonl:2` (`received`, actual `initialize`) complete raw record, with only `/home/<user>` redacted:

```json
{"ts": "2026-10-28T05:24:35.809941+00:00", "pid": 3552547, "event": "received", "data": {"jsonrpc": "2.0", "method": "initialize", "params": {"capabilities": {"general": {"positionEncodings": ["utf-8", "utf-32", "utf-16"]}, "textDocument": {"codeAction": {"codeActionLiteralSupport": {"codeActionKind": {"valueSet": ["", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source", "source.organizeImports"]}}, "dataSupport": true, "disabledSupport": true, "isPreferredSupport": true, "resolveSupport": {"properties": ["edit", "command"]}}, "completion": {"completionItem": {"deprecatedSupport": true, "insertReplaceSupport": true, "resolveSupport": {"properties": ["documentation", "detail", "additionalTextEdits"]}, "snippetSupport": true, "tagSupport": {"valueSet": [1]}}, "completionItemKind": {}}, "formatting": {"dynamicRegistration": false}, "hover": {"contentFormat": ["markdown"]}, "inlayHint": {"dynamicRegistration": false}, "publishDiagnostics": {"tagSupport": {"valueSet": [1, 2]}, "versionSupport": true}, "rename": {"dynamicRegistration": false, "honorsChangeAnnotations": false, "prepareSupport": true}, "signatureHelp": {"signatureInformation": {"activeParameterSupport": true, "documentationFormat": ["markdown"], "parameterInformation": {"labelOffsetSupport": true}}}}, "window": {"workDoneProgress": true}, "workspace": {"applyEdit": true, "configuration": true, "didChangeConfiguration": {"dynamicRegistration": false}, "didChangeWatchedFiles": {"dynamicRegistration": true, "relativePatternSupport": false}, "executeCommand": {"dynamicRegistration": false}, "fileOperations": {"didRename": true, "willRename": true}, "inlayHint": {"refreshSupport": false}, "symbol": {"dynamicRegistration": false}, "workspaceEdit": {"documentChanges": true, "failureHandling": "abort", "normalizesLineEndings": false, "resourceOperations": ["create", "rename", "delete"]}, "workspaceFolders": true}}, "clientInfo": {"name": "helix", "version": "25.07.1"}, "processId": 3552534, "rootPath": "[redacted home]/.local/state/magit-helix-probe/helix-lsp-probe", "rootUri": "file://[redacted home]/.local/state/magit-helix-probe/helix-lsp-probe", "workspaceFolders": [{"name": "helix-lsp-probe", "uri": "file://[redacted home]/.local/state/magit-helix-probe/helix-lsp-probe"}]}, "id": 0}}
```

`build/helix_lsp_probe.jsonl:13` (`received`, actual `textDocument/didChange`), same redaction:

```json
{"ts": "2026-10-28T05:25:07.751771+00:00", "pid": 3552547, "event": "received", "data": {"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {"contentChanges": [{"range": {"end": {"character": 0, "line": 0}, "start": {"character": 0, "line": 0}}, "text": "i"}], "textDocument": {"uri": "file://[redacted home]/.local/state/magit-helix-probe/helix-lsp-probe/demo.mgprobe", "version": 1}}}}
```

The initialize record includes the full `params.capabilities`, `clientInfo` (`helix`, `25.07.1`), process/root/workspace values. Independent capture `build/test_helix_lsp.jsonl:3` also has actual initialize and `:7-8` actual didChange. The fixture `tests/fixtures/lsp_traces/reconstructed-plain.jsonl` is explicitly reconstructed, not a trace.

### Plain Neovim `vim.lsp.start`, no plugin: actual native log

`build/ui/plain-render/with-final-newline/state/nvim/lsp.log:3` is the full 5,742-byte actual Neovim `init_params` record. It is copied verbatim (full capabilities and `clientInfo` included); this is Lua `vim.inspect` text, not JSON:

```text
[TRACE][2026-10-31 09:50:32] ...-unwrapped-0.12.6/share/nvim/runtime/lua/vim/lsp/log.lua:150	"LSP[magit-lsp-plain-smoke]"	"init_params"	{ capabilities = { general = { positionEncodings = { "utf-8", "utf-16", "utf-32" } }, textDocument = { callHierarchy = { dynamicRegistration = false }, codeAction = { codeActionLiteralSupport = { codeActionKind = { valueSet = { "", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source", "source.organizeImports" } } }, dataSupport = true, disabledSupport = true, dynamicRegistration = true, honorsChangeAnnotations = true, isPreferredSupport = true, resolveSupport = { properties = { "edit", "command" } } }, codeLens = { dynamicRegistration = false, resolveSupport = { properties = { "command" } } }, colorProvider = { dynamicRegistration = true }, completion = { completionItem = { commitCharactersSupport = false, deprecatedSupport = true, documentationFormat = { "markdown", "plaintext" }, insertReplaceSupport = true, labelDetailsSupport = true, preselectSupport = false, resolveSupport = { properties = { "additionalTextEdits", "command", "documentation" } }, snippetSupport = true, tagSupport = { valueSet = { 1 } } }, completionItemKind = { valueSet = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25 } }, completionList = { itemDefaults = { "editRange", "insertTextFormat", "insertTextMode", "data" } }, contextSupport = true, dynamicRegistration = false }, declaration = { linkSupport = true }, definition = { dynamicRegistration = true, linkSupport = true }, diagnostic = { dataSupport = true, dynamicRegistration = true, relatedDocumentSupport = true, relatedInformation = true, tagSupport = { valueSet = { 1, 2 } } }, documentHighlight = { dynamicRegistration = false }, documentLink = { dynamicRegistration = false, tooltipSupport = false }, documentSymbol = { dynamicRegistration = false, hierarchicalDocumentSymbolSupport = true, symbolKind = { valueSet = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26 } }, tagSupport = { valueSet = { 1 } } }, foldingRange = { dynamicRegistration = false, foldingRange = { collapsedText = true }, foldingRangeKind = { valueSet = { "comment", "imports", "region" } }, lineFoldingOnly = true }, formatting = { dynamicRegistration = true }, hover = { contentFormat = { "markdown", "plaintext" }, dynamicRegistration = true }, implementation = { linkSupport = true }, inlayHint = { dynamicRegistration = true, resolveSupport = { properties = { "textEdits", "tooltip", "command", "label.location", "label.tooltip", "label.command" } } }, inlineCompletion = { dynamicRegistration = false }, linkedEditingRange = { dynamicRegistration = false }, onTypeFormatting = { dynamicRegistration = false }, publishDiagnostics = { dataSupport = true, relatedInformation = true, tagSupport = { valueSet = { 1, 2 } } }, rangeFormatting = { dynamicRegistration = true, rangesSupport = true }, references = { dynamicRegistration = false }, rename = { dynamicRegistration = true, honorsChangeAnnotations = true, prepareSupport = true }, selectionRange = { dynamicRegistration = false }, semanticTokens = { augmentsSyntaxTokens = true, dynamicRegistration = false, formats = { "relative" }, multilineTokenSupport = true, overlappingTokenSupport = true, requests = { full = { delta = true }, range = true }, serverCancelSupport = false, tokenModifiers = { "declaration", "definition", "readonly", "static", "deprecated", "abstract", "async", "modification", "documentation", "defaultLibrary" }, tokenTypes = { "namespace", "type", "class", "enum", "interface", "struct", "typeParameter", "parameter", "variable", "property", "enumMember", "event", "function", "method", "macro", "keyword", "modifier", "comment", "string", "number", "regexp", "operator", "decorator" } }, signatureHelp = { dynamicRegistration = false, signatureInformation = { activeParameterSupport = true, documentationFormat = { "markdown", "plaintext" }, noActiveParameterSupport = true, parameterInformation = { labelOffsetSupport = true } } }, synchronization = { didSave = true, dynamicRegistration = false, willSave = true, willSaveWaitUntil = true }, typeDefinition = { linkSupport = true } }, window = { showDocument = { support = true }, showMessage = { messageActionItem = { additionalPropertiesSupport = true } }, workDoneProgress = true }, workspace = { applyEdit = true, codeLens = { refreshSupport = true }, configuration = true, diagnostics = { refreshSupport = true }, didChangeConfiguration = { dynamicRegistration = false }, didChangeWatchedFiles = { dynamicRegistration = false, relativePatternSupport = true }, executeCommand = { dynamicRegistration = false }, fileOperations = { didCreate = false, didDelete = false, didRename = false, dynamicRegistration = false, willCreate = false, willDelete = false, willRename = false }, inlayHint = { refreshSupport = true }, semanticTokens = { refreshSupport = true }, symbol = { dynamicRegistration = false, symbolKind = { valueSet = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26 } } }, workspaceEdit = { changeAnnotationSupport = { groupsOnLabel = true }, normalizesLineEndings = true, resourceOperations = { "rename", "create", "delete" } }, workspaceFolders = true } }, clientInfo = { name = "Neovim", version = "0.12.6" }, processId = 2591173, rootPath = "/tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo", rootUri = "file:///tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo", trace = "off", workDoneToken = "1", workspaceFolders = { { name = "/tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo", uri = "file:///tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo" } } }
```

Actual client `didChange`, `build/ui/plain-render/with-final-newline/state/nvim/lsp.log:18` (verbatim log record; no home/user path occurs in this record):

```text
[DEBUG][2026-10-31 09:50:33] ...-unwrapped-0.12.6/share/nvim/runtime/lua/vim/lsp/log.lua:150	"rpc.send"	{ jsonrpc = "2.0", method = "textDocument/didChange", params = { contentChanges = { { range = { ["end"] = { character = 27, line = 0 }, start = { character = 0, line = 0 } }, rangeLength = 27, text = "Head: master\n\nStaged changes (0)\n\nUnstaged changes (0)\n\nUntracked files (0)\n\nWorking tree clean" } }, textDocument = { uri = "file:///tmp/nix-shell.XA9hXT/plain-render-adtet5rf/repo/.magit", version = 4 } } }
```

`build/ui/plain-render/without-final-newline/state/nvim/lsp.log:3,18` independently records the other EOF variant. No JSON reconstruction is claimed: the observed log uses Lua table syntax; full raw line retained above.

### Bundled clients: NOT FOUND in searched artifacts

No actual bundled Neovim/VSCode initialize or didChange capture was located by the inventory and grep commands above. `build/test-output/trace-lua.jsonl` is 13 lines and has no initialize/didChange/clientInfo/capabilities matches. The two plain-render Neovim logs are plain `vim.lsp.start` runs, not bundled-plugin captures. No params were synthesized from client configuration. Helix is present above; no Helix evidence is inferred from the reconstructed fixture.

## Explicitly unobtained items / reasons

- **Bundled Neovim and bundled VSCode initialize/didChange captures: NOT FOUND**
  in the searched artifacts listed in E. Their configurations are source evidence,
  not substitute wire traces. No requests were synthesized.
- **Plain Neovim raw initialize JSON: NOT FOUND.** The available actual capture
  is a complete Lua `vim.inspect` record, reproduced in E with its full
  capabilities and clientInfo. It is deliberately not labelled raw wire JSON.
- **Real VSCode client→server status-open wire excerpt: NOT FOUND** in the
  searched logs (B/E). Source tracing settings and launcher paths are recorded;
  no fresh VSCode host capture was obtained for this report.
- **Optional xvfb/CDP native Save/Don't Save dialog DOM experiment: NOT RUN.**
  No conclusion about that dialog's DOM accessibility is inferred from source.
- **`docs/plain-support.md`: NOT FOUND.** Planned prompt/select/draft support
  and remaining Plain acceptance must not be inferred from the existing rich
  protocol types. Section C separates implemented behavior from pending work.

Only the report and the mandatory orchestration entry/checkoff in `TASKS.md`
belong to this task. Pre-existing release changes were preserved. No images were
produced or read; image-related paths in this report are text search evidence only.
