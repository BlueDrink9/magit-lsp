# Harness and client-abstraction evidence (A–E)

> Historical evidence only. Prior visual reports quoted below are not current acceptance. Current evidence policy: [AGENTS.md no-vision policy](../../AGENTS.md#working-conventions).
> Historical command paths are normalized below; evidence was not rerun.

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

