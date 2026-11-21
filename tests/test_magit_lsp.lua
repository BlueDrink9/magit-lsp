local MiniTest = require("mini.test")
local child = MiniTest.new_child_neovim()

local project = vim.fn.getcwd()
local repo

local function sys(cmd)
  local out = vim.fn.system(cmd)
  if vim.v.shell_error ~= 0 then
    error("command failed: " .. vim.inspect(cmd) .. "\n" .. out, 2)
  end
  return out
end

--- Run `git` in the fixture repository and return its stdout.
local function git_out(args)
  return sys(vim.list_extend({ "git", "-C", repo }, args))
end

local function make_repo(with_changes, path)
  if with_changes == nil then
    with_changes = true
  end
  path = path or (project .. "/build/test-repos/fixture")
  sys({ "rm", "-rf", path })
  vim.fn.mkdir(path, "p")
  sys({ "git", "init", "-q", "-b", "main", path })
  sys({ "git", "-C", path, "config", "user.email", "t@example.com" })
  sys({ "git", "-C", path, "config", "user.name", "tester" })

  local lines = {}
  for i = 1, 10 do
    table.insert(lines, "line" .. i)
  end
  vim.fn.writefile(lines, path .. "/f.txt")

  sys({ "git", "-C", path, "add", "." })
  sys({ "git", "-C", path, "commit", "-q", "-m", "init" })

  if with_changes then
    local changed = {}
    for i = 1, #lines do
      if i == 3 then
        table.insert(changed, "line3 CHANGED")
      elseif i ~= 8 then
        table.insert(changed, lines[i])
      end
      if i == 5 then
        table.insert(changed, "NEW line")
      end
    end
    vim.fn.writefile(changed, path .. "/f.txt")
  end

  return path
end

local function setup_child(opts)
  local init = { root_dir = repo }
  for key, value in pairs(opts or {}) do
    init[key] = value
  end
  child.lua(
    string.format(
      "VIEW = require('magit-lsp.view'); M = require('magit-lsp'); M.setup(%s); "
        .. "vim.wait(5000, function() local v = M.status_view(); "
        .. "return v ~= nil and v.version ~= nil and v.keymap_ready end, 10); "
        .. "STATUS = M.status_view()",
      vim.inspect(init)
    )
  )
  child.lua([[
    HELPERS = {
      lines = function()
        return vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)
      end,
      has_line = function(s)
        return vim.tbl_contains(HELPERS.lines(), s)
      end,
      line_of = function(text)
        for i, line in ipairs(HELPERS.lines()) do
          if line == text then
            return i
          end
        end
        return nil
      end,
      --- The server's outline for the focused view, never rendered text.
      --- Positions come from `textDocument/documentSymbol`; the product client
      --- The outline is requested from the server for native editor navigation.
      ---@return table[]
      symbols = function()
        local record = require('magit-lsp.view').current()
        if not record then
          return {}
        end
        local client = vim.lsp.get_client_by_id(record.client_id)
        if not client then
          return {}
        end
        local done, result
        client:request('textDocument/documentSymbol', {
          textDocument = { uri = require('magit-lsp.view').uri(record) },
        }, function(_, value)
          result = value
          done = true
        end, record.buf)
        vim.wait(5000, function()
          return done
        end, 10)
        return result or {}
      end,
      --- The section symbol named for `section` (`staged`, `unstaged`, `untracked`).
      ---@param section string
      ---@return table|nil
      section_symbol = function(section)
        local names = {
          staged = 'Staged changes',
          unstaged = 'Unstaged changes',
          untracked = 'Untracked files',
        }
        local wanted = names[section]
        for _, symbol in ipairs(HELPERS.symbols()) do
          if symbol.name == wanted then
            return symbol
          end
        end
        return nil
      end,
      --- First and last buffer line (1-based) of a section's symbol range.
      ---@param section string
      ---@return integer|nil first, integer|nil last
      section_range = function(section)
        local symbol = HELPERS.section_symbol(section)
        if not symbol then
          return nil
        end
        return symbol.range.start.line + 1, symbol.range['end'].line + 1
      end,
      find_line = function(marker, text, section)
        local first, last = HELPERS.section_range(section)
        if not first then
          return nil
        end
        local wanted = '    ' .. marker .. text
        local lines = HELPERS.lines()
        for i = first, last do
          if lines[i] == wanted then
            return i
          end
        end
        return nil
      end,
      find_first = function(kind, section)
        local symbol = HELPERS.section_symbol(section)
        if not symbol then
          return nil
        end
        if kind == 'section' then
          return symbol.selectionRange.start.line + 1
        end
        local children = symbol.children or {}
        if kind == 'file' then
          local file = children[1]
          return file and (file.selectionRange.start.line + 1) or nil
        end
        if kind == 'hunk' then
          for _, file in ipairs(children) do
            local hunk = (file.children or {})[1]
            if hunk then
              return hunk.selectionRange.start.line + 1
            end
          end
        end
        return nil
      end,
      token_at = function(row, col)
        local tokens = vim.lsp.semantic_tokens.get_at_pos(STATUS.buf, row, col)
        return tokens and tokens[1] or nil
      end,
      token_type_on = function(text, col)
        for i, line in ipairs(STATUS.lines) do
          if line == text then
            local token = HELPERS.token_at(i - 1, col or 0)
            return token and token.type or nil
          end
        end
        return nil
      end,
      wait_tokens = function()
        return vim.wait(5000, function()
          for i = 0, #STATUS.lines - 1 do
            if HELPERS.token_at(i, 0) then
              return true
            end
          end
          return false
        end, 20)
      end,
      legend = function()
        local client = vim.lsp.get_client_by_id(STATUS.client_id)
        return client.server_capabilities.semanticTokensProvider.legend
      end,
      missing_links = function()
        local legend = HELPERS.legend()
        local missing = {}
        local function check(group)
          local hl = vim.api.nvim_get_hl(0, { name = group, link = true })
          if not hl.link or hl.fg or hl.bg then
            missing[#missing + 1] = group
          end
        end
        for _, token_type in ipairs(legend.tokenTypes) do
          check('@lsp.type.' .. token_type .. '.magit-lsp')
        end
        for _, modifier in ipairs(legend.tokenModifiers) do
          check('@lsp.mod.' .. modifier .. '.magit-lsp')
        end
        return missing
      end,
      resolved_fg = function(token_type)
        local group = '@lsp.type.' .. token_type .. '.magit-lsp'
        return vim.api.nvim_get_hl(0, { name = group, link = false }).fg
      end,
      resync = function()
        local client = vim.lsp.get_client_by_id(STATUS.client_id)
        local done, result
        client:request('view/resync', { viewId = STATUS.id }, function(err, value)
          result = value
          done = true
        end, STATUS.buf)
        vim.wait(5000, function()
          return done
        end, 10)
        return result
      end,
      cursor_to = function(line)
        vim.api.nvim_win_set_cursor(0, { line, 0 })
      end,
      normalize_lhs = function(lhs, mode)
        local buf = vim.api.nvim_create_buf(false, true)
        vim.api.nvim_buf_set_keymap(buf, mode or 'n', lhs, '', {})
        local map = vim.api.nvim_buf_get_keymap(buf, mode or 'n')[1]
        vim.api.nvim_buf_delete(buf, { force = true })
        return map and map.lhs
      end,
      applied_lhs = function(mode)
        local out = {}
        for _, map in ipairs(vim.api.nvim_buf_get_keymap(STATUS.buf, mode or 'n')) do
          out[map.lhs] = true
        end
        return out
      end,
      expected_keymap = function(notice, mode)
        mode = mode or 'n'
        local keys = require('magit-lsp.keys')
        local want = mode == 'n' and 'normal' or 'visual'
        local text_mode = notice.mode == 'text'
        local out = {}
        local visit_implemented = false
        for _, binding in ipairs(notice.bindings or {}) do
          if binding.command == 'common.visit' and binding.status == 'implemented' then
            visit_implemented = true
          end
          local modes = binding.modes or { 'normal', 'visual' }
          if vim.tbl_contains(modes, want) then
            for _, chord in ipairs(binding.keys or {}) do
              out[HELPERS.normalize_lhs(keys.to_lhs(chord), mode)] = true
            end
          end
        end
        local visit = not text_mode and not visit_implemented and notice.scope == 'status'
        if visit then
          out[HELPERS.normalize_lhs('<CR>', mode)] = (mode == 'n') or nil
        end
        if mode == 'n' and not text_mode then
          for _, native in ipairs({ 'za', 'zA', 'zo', 'zO', 'zc', 'zC', 'zR', 'zM', 'zr', 'zm', 'zj', 'zk' }) do
            out[HELPERS.normalize_lhs(native, mode)] = true
          end
        end
        return out
      end,
      diff_keys = function(got, want)
        local missing, extra = {}, {}
        for key in pairs(want) do
          if not got[key] then
            missing[#missing + 1] = key
          end
        end
        for key in pairs(got) do
          if not want[key] then
            extra[#extra + 1] = key
          end
        end
        table.sort(missing)
        table.sort(extra)
        return missing, extra
      end,
      keymap_diff = function(mode)
        local want = HELPERS.expected_keymap(STATUS.keymap, mode or 'n')
        local got = HELPERS.applied_lhs(mode or 'n')
        local missing, extra = HELPERS.diff_keys(got, want)
        return { missing = missing, extra = extra }
      end,
      --- Poll `fn` with a bounded `vim.wait`, yielding the Neovim event loop
      --- so queued input, LSP notifications and scheduled callbacks can run.
      --- Returns false on timeout so the caller's assertion reports the
      --- actual state instead of concealing it; never hangs.
      ---@param fn fun(): boolean
      ---@param timeout integer|nil milliseconds, default 5000
      ---@return boolean
      wait_for = function(fn, timeout)
        return vim.wait(timeout or 5000, fn, 20)
      end,
      --- Poll until the magit-lsp client is idle: no outstanding requests,
      --- prompts, progress tokens, scheduled work or unacknowledged renders.
      --- Idle must hold for a short settle period: the server sends a response
      --- before the render it produced, so the first idle reading can race a
      --- notification that is still in flight.
      ---@return boolean
      wait_idle = function()
        local since = nil
        return HELPERS.wait_for(function()
          if not require('magit-lsp.debug').idle() then
            since = nil
            return false
          end
          if since == nil then
            since = vim.uv.now()
            return false
          end
          return vim.uv.now() - since >= 50
        end)
      end,
    }
  ]])
end

local function assert_line(needle)
  local has = child.lua_get(string.format("HELPERS.has_line(%q)", needle))
  MiniTest.expect.equality(has, true, needle)
end

local function assert_no_line(needle)
  local has = child.lua_get(string.format("HELPERS.has_line(%q)", needle))
  MiniTest.expect.equality(has, false, needle)
end

--- Place the cursor on the first matching rendered line and invoke a command.
local function invoke_at(line, command, opts)
  local call = string.format(
    "vim.api.nvim_win_set_cursor(0, {%d, 0}); R = M.invoke(%q%s)",
    line,
    command,
    opts and (", " .. opts) or ""
  )
  -- The product client is asynchronous; the test waits for its own idle.
  child.lua(call .. "; HELPERS.wait_idle()")
  return child.lua_get("R ~= nil")
end

-- Type keys against the read-only status buffer (TASKS.md IMM-4). Every
-- attempt must leave the full buffer text and `modified=false` untouched.
-- `type_keys()` raises when `v:errmsg` is set, so a mapped path must never
-- surface a Lua traceback; a key unbound in the active profile must fail
-- natively with E21 and never silently. Stale `v:errmsg` is cleared before
-- the input so the observed error belongs to this attempt.
--
-- Mapped keys are distinguished honestly: `u` is bound to unstage (a no-op
-- on the header), so it is passed `{ bound_noop = true }` and must produce
-- NO E21 — claiming E21 there would be a false assertion. IMM-4's `dd`/`x`
-- deviation: `d` opens the diff menu and `x` discards, so neither is typed
-- as a native edit here. `c` opens the commit view, so `ciw<Esc>` is NOT
-- unbound; the native `ciw` path is exercised with mappings deliberately
-- bypassed (`{ native = true }`, feedkeys noremap) and covered by the
-- non-vacuity check below, where the same bypass makes the guard fail.
local function type_edit(keys, opts)
  opts = opts or {}
  -- Clear stale `v:errmsg` before typing so the poll below observes this
  -- attempt's error only.
  child.v.errmsg = ""
  local before = child.lua_get("vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)")
  local ok, err, after, modified, native_errmsg
  if opts.native then
    -- Deliberately bypass mappings to exercise the native command itself;
    -- expand termcodes, then `x` (execute now) + `n` (noremap).
    -- The control edits, snapshots and restores in one child call without
    -- yielding to live renders. Cleanup runs even if input/snapshotting fails.
    local result = child.lua_get(string.format(
      [[
      (function()
        local buf = STATUS.buf
        local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
        local modifiable, modified = vim.bo[buf].modifiable, vim.bo[buf].modified
        local cursor, errmsg = vim.api.nvim_win_get_cursor(0), vim.v.errmsg
        local control = %s
        local success, result = pcall(function()
          local input = vim.api.nvim_replace_termcodes(%q, true, false, true)
          if control then vim.bo[buf].modifiable = true end
          local ok, err = pcall(vim.fn.feedkeys, input, 'xn')
          return {
            ok = ok, err = tostring(err), errmsg = vim.v.errmsg, before = lines,
            after = vim.api.nvim_buf_get_lines(buf, 0, -1, false),
            modified = vim.bo[buf].modified,
          }
        end)
        if control then
          vim.bo[buf].modifiable = true
          local restored, restore_err = pcall(vim.api.nvim_buf_set_lines, buf, 0, -1, false, lines)
          vim.bo[buf].modifiable, vim.bo[buf].modified = modifiable, modified
          vim.api.nvim_win_set_cursor(0, cursor)
          vim.v.errmsg = errmsg
          if not restored then error(restore_err) end
        end
        if not success then error(result) end
        return result
      end)()
    ]],
      tostring(opts.control == true),
      keys
    ))
    ok, err, after, modified = result.ok, result.err, result.after, result.modified
    before = result.before
    -- Keep the observed error even though the control restored v:errmsg.
    native_errmsg = result.errmsg
  else
    ok, err = pcall(child.type_keys, 50, keys)
  end
  -- Mapped input may queue work; native feedkeys above executes synchronously.
  -- Never sleep: poll for the mapped attempt's error before classifying it.
  if not opts.native then
    child.lua_get("HELPERS.wait_for(function() return vim.v.errmsg ~= '' end, 500)")
  end
  local errmsg = native_errmsg or child.v.errmsg
  child.v.errmsg = ""
  -- Any command the refused key mapped to (e.g. `u` on the header) must have
  -- drained before the next step, so a pending render cannot leak out.
  if not opts.control then
    child.lua_get("HELPERS.wait_idle()")
  end
  -- Independent guards, buffer state first: a silently successful edit (e.g.
  -- the negative control while temporarily modifiable) must be reported as an
  -- unchanged-buffer violation even though no E21 can occur; the E21
  -- classification below then still covers every refusal path, so neither
  -- check is weakened.
  after = after or child.lua_get("vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)")
  if modified == nil then
    modified = child.lua_get("vim.api.nvim_get_option_value('modified', { buf = STATUS.buf })")
  end
  MiniTest.expect.equality(
    after,
    before,
    { fail_reason = "buffer must be unchanged after " .. keys }
  )
  MiniTest.expect.equality(
    modified,
    false,
    { fail_reason = "buffer must stay unmodified after " .. keys }
  )
  if opts.bound_noop then
    -- A mapped no-op must succeed, not masquerade as a native refusal.
    MiniTest.expect.equality(ok, true, { fail_reason = tostring(err) })
    MiniTest.expect.equality(
      errmsg,
      "",
      { fail_reason = "mapped no-op must not set an error: " .. errmsg }
    )
  elseif not ok then
    -- A mapped path must fail natively, never with a Lua traceback.
    MiniTest.expect.equality(
      tostring(err):find("E21", 1, true) ~= nil,
      true,
      { fail_reason = "unexpected error: " .. tostring(err) }
    )
  else
    -- An unbound native edit must actually be refused with E21.
    MiniTest.expect.equality(
      errmsg:find("E21", 1, true) ~= nil,
      true,
      { fail_reason = "unexpected error: " .. errmsg }
    )
  end
end

local T = MiniTest.new_set({
  hooks = {
    pre_once = function()
      sys({ "cargo", "build" })
    end,
    pre_case = function()
      repo = make_repo()
      child.restart({ "-u", "scripts/minimal_init.lua" })
    end,
    post_case = function()
      pcall(
        child.lua,
        [[if M then
          require('magit-lsp.view').close_all()
          for _, session in pairs(M.sessions) do
            local c = vim.lsp.get_client_by_id(session.client_id)
            if c then c:stop(true) end
          end
        end]]
      )
      child.stop()
    end,
  },
})

T["plugin loading registers :Magit without setup"] = function()
  child.lua([[
    vim.cmd('runtime plugin/magit-lsp.lua')
    REGISTERED = vim.fn.exists(':Magit')
    SESSIONS = vim.tbl_count(require('magit-lsp').sessions)
    CLIENTS = #vim.lsp.get_clients({ name = 'magit-lsp' })
  ]])
  MiniTest.expect.equality(child.lua_get("REGISTERED"), 2)
  MiniTest.expect.equality(child.lua_get("SESSIONS"), 0, "loading the plugin must not call setup()")
  MiniTest.expect.equality(
    child.lua_get("CLIENTS"),
    0,
    "loading the plugin must not start a server"
  )
end

T[":h magit resolves the shipped help file"] = function()
  local help_dir = project .. "/build/test-help-magit"
  child.lua(string.format(
    [[
    local help_dir = %q
    vim.fn.delete(help_dir, 'rf')
    vim.fn.mkdir(help_dir .. '/doc', 'p')
    vim.fn.writefile(vim.fn.readfile(%q .. '/doc/magit.txt'), help_dir .. '/doc/magit.txt')
    vim.cmd('helptags ' .. vim.fn.fnameescape(help_dir .. '/doc'))
    vim.opt.runtimepath:append(help_dir)
    vim.cmd('help magit')
    local stack = vim.fn.gettagstack(0).items
    local last = stack[#stack]
    HELP = {
      file = vim.api.nvim_buf_get_name(0),
      tag = last and last.tagname,
      lines = vim.api.nvim_buf_get_lines(0, 0, -1, false),
      tags_generated = vim.fn.filereadable(help_dir .. '/doc/tags') == 1,
      repo_tags = vim.fn.filereadable(%q .. '/doc/tags') == 1,
      help_buffer = vim.bo.filetype == 'help',
    }
  ]],
    help_dir,
    project,
    project
  ))
  local file = child.lua_get("HELP.file")
  local lines = child.lua_get("HELP.lines")
  local function has_prefix(prefix)
    for _, line in ipairs(lines) do
      if line:sub(1, #prefix) == prefix then
        return true
      end
    end
    return false
  end
  local text = table.concat(lines, "\n")
  MiniTest.expect.equality(file, help_dir .. "/doc/magit.txt")
  MiniTest.expect.equality(((child.lua_get("HELP.tag") or ""):gsub("@.*$", "")), "magit")
  MiniTest.expect.equality(child.lua_get("HELP.tags_generated"), true)
  MiniTest.expect.equality(child.lua_get("HELP.repo_tags"), false, "no doc/tags in the repository")
  MiniTest.expect.equality(child.lua_get("HELP.help_buffer"), true)
  MiniTest.expect.equality(
    has_prefix(":Magit [directory]"),
    true,
    "help documents :Magit [directory]"
  )
  MiniTest.expect.equality(
    text:find("Open the status view.  Without an argument, focus the current", 1, true) ~= nil,
    true,
    "help documents the default-root behavior"
  )
  for _, needle in ipairs({ "*magit*", "$MAGIT_LSP_BIN", "docs/KEYMAPS.md", "protocol mismatch" }) do
    MiniTest.expect.equality(
      text:find(needle, 1, true) ~= nil,
      true,
      "help text mentions " .. needle
    )
  end
  child.lua(string.format("vim.fn.delete(%q, 'rf')", help_dir))
end

T["the :Magit command delegates to the public open path"] = function()
  repo = make_repo(true, project .. "/build/test-repos/dir with space")
  setup_child()
  child.lua(string.format(
    [[
    vim.cmd('runtime plugin/magit-lsp.lua')
    local calls = {}
    local client = require('magit-lsp')
    local real = client.open
    client.open = function(root)
      calls[#calls + 1] = root == nil and 'default' or root
      return real(root)
    end
    vim.cmd('Magit ' .. vim.fn.fnameescape(%s))
    FOCUSED_DIRECTORY = vim.api.nvim_get_current_buf() == STATUS.buf
    vim.cmd('Magit')
    FOCUSED_DEFAULT = vim.api.nvim_get_current_buf() == STATUS.buf
    client.open = real
    CALLS = calls
  ]],
    vim.inspect(repo)
  ))
  MiniTest.expect.equality(child.lua_get("FOCUSED_DIRECTORY"), true)
  MiniTest.expect.equality(child.lua_get("FOCUSED_DEFAULT"), true)
  MiniTest.expect.equality(child.lua_get("CALLS"), { repo, "default" })
end

T["bare :Magit starts a session for the current directory without setup"] = function()
  child.lua(string.format("vim.cmd('cd ' .. vim.fn.fnameescape(%q))", repo))
  child.cmd("runtime plugin/magit-lsp.lua")
  child.cmd("Magit")
  local state = child.lua_get([[
    (function()
      M = require('magit-lsp')
      local opened = vim.wait(10000, function()
        local record = M.status_view()
        return record ~= nil and record.version ~= nil and record.keymap_ready
      end, 20)
      local record = M.status_view()
      local session = M.sessions[vim.fn.getcwd()]
      return {
        opened = opened,
        session = session ~= nil,
        clients = #vim.lsp.get_clients({ name = 'magit-lsp' }),
        filetype = record and vim.api.nvim_get_option_value('filetype', { buf = record.buf }) or nil,
        focused = record ~= nil and vim.api.nvim_get_current_buf() == record.buf,
        lines = record and vim.api.nvim_buf_get_lines(record.buf, 0, -1, false) or {},
      }
    end)()
  ]])
  MiniTest.expect.equality(state.opened, true, "bare :Magit opened a status view")
  MiniTest.expect.equality(state.session, true, "the cwd repository has a session")
  MiniTest.expect.equality(state.clients, 1, "exactly one server started")
  MiniTest.expect.equality(state.filetype, "magit-lsp")
  MiniTest.expect.equality(state.focused, true, "the status view is focused")
  MiniTest.expect.equality(
    vim.tbl_contains(state.lines, "Unstaged changes (1)"),
    true,
    "the server rendered the repository status"
  )
  MiniTest.expect.equality(
    vim.tbl_contains(state.lines, "    +NEW line"),
    true,
    "the server rendered the diff"
  )
end

T["bare :Magit reopens a wiped status without a second server"] = function()
  setup_child({ fold_keys = { ["<F3>"] = "common.toggle-section" } })
  child.lua(string.format("vim.cmd('cd ' .. vim.fn.fnameescape(%q))", repo))
  child.lua([[
    M = require('magit-lsp')
    SESSION = M.sessions[vim.fn.getcwd()]
    CLIENT = SESSION.client_id
    CLIENTS_BEFORE = #vim.lsp.get_clients({ name = 'magit-lsp' })
    vim.api.nvim_buf_delete(STATUS.buf, { force = true })
  ]])
  MiniTest.expect.equality(
    child.lua_get("M.status_view() == nil"),
    true,
    "the status view was wiped"
  )

  child.cmd("Magit")
  local state = child.lua_get([[
    (function()
      local opened = vim.wait(10000, function()
        local record = M.status_view()
        return record ~= nil and record.version ~= nil and record.keymap_ready
      end, 20)
      STATUS = M.status_view()
      local session = M.sessions[vim.fn.getcwd()]
      return {
        opened = opened,
        same_session = session == SESSION,
        same_client = session.client_id == CLIENT,
        clients = #vim.lsp.get_clients({ name = 'magit-lsp' }),
        clients_before = CLIENTS_BEFORE,
        focused = STATUS ~= nil and vim.api.nvim_get_current_buf() == STATUS.buf,
      }
    end)()
  ]])
  MiniTest.expect.equality(state.opened, true, "bare :Magit reopened the status view")
  MiniTest.expect.equality(state.same_session, true, "the configured session is reused")
  MiniTest.expect.equality(state.same_client, true, "no duplicate server was started")
  MiniTest.expect.equality(state.clients, state.clients_before, "the client count is unchanged")
  MiniTest.expect.equality(state.focused, true, "the reopened status view is focused")
  MiniTest.expect.equality(
    child.lua_get("HELPERS.applied_lhs('n')['<F3>'] ~= nil"),
    true,
    "the session's configured fold key survived the reopen"
  )
  assert_line("Unstaged changes (1)")
  assert_line("    +NEW line")
end

T[":Magit starts an explicit repository directory"] = function()
  local other = make_repo(true, project .. "/build/test-repos/explicit start")
  vim.fn.writefile({ "other" }, other .. "/other.txt")
  child.cmd("runtime plugin/magit-lsp.lua")
  child.cmd("Magit " .. vim.fn.fnameescape(other))
  local state = child.lua_get(string.format(
    [[
    (function()
      M = require('magit-lsp')
      local opened = vim.wait(10000, function()
        local record = M.status_view(%q)
        return record ~= nil and record.version ~= nil and record.keymap_ready
      end, 20)
      local record = M.status_view(%q)
      return {
        opened = opened,
        session = M.sessions[%q] ~= nil,
        sessions = vim.tbl_count(M.sessions),
        clients = #vim.lsp.get_clients({ name = 'magit-lsp' }),
        focused = record ~= nil and vim.api.nvim_get_current_buf() == record.buf,
        lines = record and vim.api.nvim_buf_get_lines(record.buf, 0, -1, false) or {},
      }
    end)()
  ]],
    other,
    other,
    other
  ))
  MiniTest.expect.equality(state.opened, true, "explicit :Magit opened a status view")
  MiniTest.expect.equality(state.session, true, "the explicit repository has a session")
  MiniTest.expect.equality(state.sessions, 1, "only the requested repository was started")
  MiniTest.expect.equality(state.clients, 1, "exactly one server started")
  MiniTest.expect.equality(state.focused, true, "the status view is focused")
  MiniTest.expect.equality(
    vim.tbl_contains(state.lines, "  ? other.txt"),
    true,
    "the server rendered the explicit repository's status"
  )
end

T["creates a server-named virtual buffer and attaches LSP"] = function()
  setup_child()
  local buftype = child.lua_get("vim.api.nvim_get_option_value('buftype', { buf = STATUS.buf })")
  MiniTest.expect.equality(buftype, "nofile")

  local bufname = child.lua_get("vim.api.nvim_buf_get_name(STATUS.buf)")
  MiniTest.expect.equality(bufname, "magit://status" .. repo)

  local attached = child.lua_get("vim.lsp.buf_is_attached(STATUS.buf, STATUS.client_id)")
  MiniTest.expect.equality(attached, true)
end

T["server version warning only reports mismatch or missing metadata"] = function()
  child.lua([[WARNINGS = {}; vim.notify = function(message) table.insert(WARNINGS, message) end
    local version = require('magit-lsp.version')
    CLIENT_VERSION = version.version
    version.check({ version = version.version }, { '/selected/magit-lsp' })
    version.check({ version = '9.9.9' }, { '/selected/magit-lsp' })
    version.check({}, { '/selected/magit-lsp' })]])
  local warnings = child.lua_get("WARNINGS")
  local client_version = child.lua_get("CLIENT_VERSION")
  local expected = string.format("client version %s, server version 9.9.9", client_version)
  MiniTest.expect.equality(#warnings, 2)
  MiniTest.expect.equality(warnings[1]:find(expected, 1, true) ~= nil, true)
  MiniTest.expect.equality(warnings[1]:find("/selected/magit-lsp", 1, true) ~= nil, true)
  MiniTest.expect.equality(warnings[2]:find("server version missing", 1, true) ~= nil, true)
end

T["declares exact bundled Neovim capabilities"] = function()
  local declaration =
    child.lua_get("require('magit-lsp.config').client_capabilities().experimental.magit")
  local shared = child.lua_get("require('magit-lsp.shared')")
  MiniTest.expect.equality(declaration, {
    v = 1,
    minimumVersion = 1,
    keys = "dynamic",
    modifiers = child.lua_get("require('magit-lsp.config').key_modifiers()"),
    layout = { "main", "side", "bottom", "float" },
    effects = shared.optional_effects,
    prompts = shared.optional_prompt_kinds,
    pick = {
      multi = false,
      freeText = false,
      description = false,
      detail = false,
      group = false,
      preselection = false,
      maxItems = 100,
      fallbackView = true,
    },
  })
end

T["the rich protocol declaration and a matching advertisement open rich traffic"] = function()
  -- anchor: lua/magit-lsp/config.lua client_capabilities; real startup below.
  child.lua([[
    DECLARED = nil
    REQUESTS = {}
    NOTIFICATIONS = {}
    WARNINGS = {}
    vim.notify = function(msg, level) table.insert(WARNINGS, { msg = msg, level = level }) end
    local root = vim.fn.tempname()
    vim.fn.mkdir(root, 'p')
    local client = require('magit-lsp')
    client.setup({
      root_dir = root,
      cmd = function()
        local rpc = {}
        function rpc.request(method, params, callback)
          REQUESTS[#REQUESTS + 1] = method
          if method == 'initialize' then
            DECLARED = params.capabilities.experimental.magit
            callback(nil, {
              capabilities = { experimental = { magit = { protocolVersion = 1 } } },
              serverInfo = { version = require('magit-lsp.version').version },
            })
          elseif method == 'view/open' then
            callback(nil, {
              id = 'fake-status',
              kind = 'status',
              uri = 'magit://fake',
              title = 'Status',
              placement = 'main',
              mode = 'interactive',
              scope = 'status',
            })
          end
          return true, 1
        end
        function rpc.notify(method)
          NOTIFICATIONS[#NOTIFICATIONS + 1] = method
        end
        function rpc.is_closing()
          return false
        end
        function rpc.terminate() end
        return rpc
      end,
    })
    OPENED = vim.wait(5000, function() return client.status_view() ~= nil end, 10)
  ]])
  MiniTest.expect.equality(
    child.lua_get("OPENED"),
    true,
    "the matching advertisement opened a rich view"
  )
  local declared = child.lua_get("DECLARED")
  MiniTest.expect.equality(declared.v, 1)
  MiniTest.expect.equality(declared.minimumVersion, 1)
  local requests = child.lua_get("REQUESTS")
  MiniTest.expect.equality(requests[1], "initialize")
  MiniTest.expect.equality(vim.tbl_contains(requests, "view/open"), true, "rich traffic was sent")
  MiniTest.expect.equality(child.lua_get("WARNINGS"), {})
end

T["a missing, malformed or different protocol advertisement is a readable rejection"] = function()
  -- anchor: lua/magit-lsp/version.lua protocol_error; the real initialize wrapper
  -- in lua/magit-lsp/init.lua is exercised below. External containers are
  -- arbitrary JSON, so a malformed one must take this same readable path
  -- instead of throwing out of the initialize callback.
  local function reject(result_expression, expected, label)
    child.lua(string.format(
      [[
      NOTIFIES = {}
      REQUESTS = {}
      NOTIFICATIONS = {}
      TERMINATED = false
      vim.notify = function(msg, level) table.insert(NOTIFIES, { msg = msg, level = level }) end
      local root = vim.fn.tempname()
      vim.fn.mkdir(root, 'p')
      local client = require('magit-lsp')
      client.setup({
        root_dir = root,
        cmd = function()
          local rpc = {}
          function rpc.request(method, params, callback)
            REQUESTS[#REQUESTS + 1] = method
            if method == 'initialize' then
              callback(nil, %s)
            end
            return true, 1
          end
          function rpc.notify(method) NOTIFICATIONS[#NOTIFICATIONS + 1] = method end
          function rpc.is_closing() return false end
          function rpc.terminate() TERMINATED = true end
          return rpc
        end,
      })
      REJECTED = vim.wait(5000, function() return #NOTIFIES > 0 end, 10)
      client.open(root)
      client.setup({ root_dir = root })
      STATUS = client.status_view()
    ]],
      result_expression
    ))

    MiniTest.expect.equality(child.lua_get("REJECTED"), true, label)
    MiniTest.expect.equality(child.lua_get("#NOTIFIES"), 1, label)
    local msg = child.lua_get("NOTIFIES[1].msg")
    MiniTest.expect.equality(msg:find("rich protocol mismatch", 1, true) ~= nil, true, msg)
    MiniTest.expect.equality(msg:find("required version 1", 1, true) ~= nil, true, msg)
    MiniTest.expect.equality(msg:find(expected, 1, true) ~= nil, true, msg)
    MiniTest.expect.equality(child.lua_get("NOTIFIES[1].level"), vim.log.levels.ERROR)
    MiniTest.expect.equality(child.lua_get("STATUS == nil"), true, "no rich view for " .. label)
    MiniTest.expect.equality(
      child.lua_get("TERMINATED"),
      true,
      "the rejected transport stopped for " .. label
    )
    MiniTest.expect.equality(
      child.lua_get("REQUESTS"),
      { "initialize" },
      "no rich traffic for " .. label
    )
    MiniTest.expect.equality(
      child.lua_get("NOTIFICATIONS"),
      {},
      "no initialized or rich notifications for " .. label
    )
  end

  local versions = {
    { value = "nil", expected = "server advertised none" },
    { value = "'1'", expected = 'server advertised "1"' },
    { value = "0", expected = "server advertised 0" },
    { value = "-1", expected = "server advertised -1" },
    { value = "1.5", expected = "server advertised 1.5" },
    { value = "2", expected = "server advertised 2" },
  }
  for _, case in ipairs(versions) do
    reject(
      string.format(
        "{ capabilities = { experimental = { magit = { protocolVersion = %s } } } }",
        case.value
      ),
      case.expected,
      case.expected
    )
  end

  local malformed = {
    { name = "boolean initialize result", result = "true" },
    { name = "numeric initialize result", result = "42" },
    { name = "null initialize result", result = "vim.NIL" },
    { name = "boolean capabilities", result = "{ capabilities = true }" },
    { name = "numeric capabilities", result = "{ capabilities = 42 }" },
    { name = "null capabilities", result = "{ capabilities = vim.NIL }" },
    { name = "boolean experimental", result = "{ capabilities = { experimental = true } }" },
    { name = "numeric experimental", result = "{ capabilities = { experimental = 42 } }" },
    { name = "null experimental", result = "{ capabilities = { experimental = vim.NIL } }" },
    { name = "boolean magit", result = "{ capabilities = { experimental = { magit = true } } }" },
    { name = "numeric magit", result = "{ capabilities = { experimental = { magit = 42 } } }" },
    { name = "null magit", result = "{ capabilities = { experimental = { magit = vim.NIL } } }" },
  }
  for _, case in ipairs(malformed) do
    reject(case.result, "server advertised none", case.name)
  end
end

T["an asynchronous initialize rejection reports the server explanation"] = function()
  -- The real server rejects a declaration that does not select rich v1. The
  -- client must report that asynchronous initialize RPC error, open no rich
  -- view and send no rich traffic.
  child.lua(string.format(
    [[
    NOTIFIES = {}
    vim.notify = function(msg, level) table.insert(NOTIFIES, { msg = msg, level = level }) end
    local config = require('magit-lsp.config')
    local capabilities = config.client_capabilities
    config.client_capabilities = function()
      local declared = capabilities()
      declared.experimental.magit.v = 2
      declared.experimental.magit.minimumVersion = 2
      return declared
    end
    local client = require('magit-lsp')
    REQUESTS = {}
    NOTIFICATIONS = {}
    EXITED = false
    client.setup({ root_dir = %q, cmd = function(dispatchers, cfg)
      local on_exit = dispatchers.on_exit
      dispatchers.on_exit = function(...)
        EXITED = true
        on_exit(...)
      end
      RPC = vim.lsp.rpc.start(config.server_command(), dispatchers, {
        cwd = cfg.cmd_cwd, env = cfg.cmd_env, detached = cfg.detached,
      })
      local request, notify = RPC.request, RPC.notify
      RPC.request = function(method, ...)
        REQUESTS[#REQUESTS + 1] = method
        return request(method, ...)
      end
      RPC.notify = function(method, ...)
        NOTIFICATIONS[#NOTIFICATIONS + 1] = method
        return notify(method, ...)
      end
      return RPC
    end })
    REJECTED = vim.wait(10000, function() return #NOTIFIES > 0 end, 20)
    STOPPED = vim.wait(5000, function()
      local session = client.sessions[%q]
      return EXITED and vim.lsp.get_client_by_id(session.client_id) == nil
    end, 10)
    STATUS = client.status_view()
    PROTOCOL = client.sessions[%q] and client.sessions[%q].protocol
  ]],
    repo,
    repo,
    repo,
    repo
  ))
  MiniTest.expect.equality(child.lua_get("REJECTED"), true, "the rejection was reported")
  MiniTest.expect.equality(child.lua_get("#NOTIFIES"), 1)
  local msg = child.lua_get("NOTIFIES[1].msg")
  MiniTest.expect.equality(msg:find("initialize failed", 1, true) ~= nil, true, msg)
  MiniTest.expect.equality(msg:find("unsupported rich protocol version", 1, true) ~= nil, true, msg)
  MiniTest.expect.equality(child.lua_get("NOTIFIES[1].level"), vim.log.levels.ERROR)
  MiniTest.expect.equality(child.lua_get("STATUS == nil"), true, "no rich view opened")
  MiniTest.expect.equality(child.lua_get("PROTOCOL"), "failed", "the session is not rich")
  MiniTest.expect.equality(
    child.lua_get("STOPPED and RPC.is_closing()"),
    true,
    "the real server exited"
  )
  MiniTest.expect.equality(child.lua_get("REQUESTS"), { "initialize" }, "no rich requests or retry")
  MiniTest.expect.equality(
    child.lua_get("NOTIFICATIONS"),
    {},
    "no initialized or rich notifications"
  )
  MiniTest.expect.equality(
    child.lua_get([[
    (function()
      for _, session in pairs(require('magit-lsp').sessions) do
        local c = vim.lsp.get_client_by_id(session.client_id)
        if c and c.initialized then return true end
      end
      return false
    end)()
  ]]),
    false,
    "the client never initialized"
  )
end

T["staging a visual line range stages only those lines"] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), "number", "change line index")
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, "V", "s")

  local staged = child.lua_get(
    "HELPERS.wait_for(function() return HELPERS.find_line('+', 'NEW line', 'staged') ~= nil end)"
  )
  MiniTest.expect.equality(staged, true)
  MiniTest.expect.equality(child.lua_get("HELPERS.wait_idle()"), true)

  local cached = git_out({ "diff", "--cached", "-U3", "--", "f.txt" })
  MiniTest.expect.equality(cached:find("+NEW line", 1, true) ~= nil, true, cached)
  MiniTest.expect.equality(cached:find("line3 CHANGED", 1, true), nil, cached)
  MiniTest.expect.equality(
    git_out({ "diff", "-U3", "--", "f.txt" }):find("line3 CHANGED", 1, true) ~= nil,
    true
  )
end

T["a failed mutation notifies once with a readable summary"] = function()
  setup_child()
  child.lua([[
    NOTIFIES = {}
    vim.notify = function(msg, level)
      table.insert(NOTIFIES, { msg = msg, level = level })
    end
  ]])
  vim.fn.writefile({}, repo .. "/.git/index.lock")
  child.lua("M.invoke('status.stage-all')")
  local notified = child.lua_get("HELPERS.wait_for(function() return #NOTIFIES > 0 end)")
  vim.fn.delete(repo .. "/.git/index.lock")

  MiniTest.expect.equality(notified, true, "the failed mutation notified")
  MiniTest.expect.equality(child.lua_get("#NOTIFIES"), 1)
  local msg = child.lua_get("NOTIFIES[1].msg")
  MiniTest.expect.equality(type(msg), "string")
  MiniTest.expect.equality(
    msg:find("Another Git process is running in this repository.", 1, true) ~= nil,
    true
  )
  MiniTest.expect.equality(msg:find("{", 1, true), nil)
  MiniTest.expect.equality(msg:find("<metatable>", 1, true), nil)
  MiniTest.expect.equality(child.lua_get("NOTIFIES[1].level"), vim.log.levels.ERROR)
end

T["semantic tokens highlight sections, files, hunks and diff lines"] = function()
  setup_child()
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('filetype', { buf = STATUS.buf })"),
    "magit-lsp"
  )
  MiniTest.expect.equality(child.lua_get("HELPERS.wait_tokens()"), true, "semantic tokens arrive")

  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('Unstaged changes (1)', 0)"),
    "magit.section"
  )
  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('  M f.txt', 2)"),
    "magit.file.modified"
  )
  MiniTest.expect.equality(child.lua_get("HELPERS.token_type_on('  M f.txt', 4)"), "magit.file")
  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('    @@ -1,10 +1,10 @@', 4)"),
    "magit.hunk.header"
  )
  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('    +line3 CHANGED', 4)"),
    "magit.diff.added"
  )
  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('    -line3', 4)"),
    "magit.diff.removed"
  )
end

T["every token type and modifier in the legend links to a theme group"] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get("HELPERS.wait_tokens()"), true, "semantic tokens arrive")
  MiniTest.expect.equality(child.lua_get("HELPERS.missing_links()"), {})
end

T["theme links survive a colorscheme switch"] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get("HELPERS.wait_tokens()"), true, "semantic tokens arrive")

  local added = child.lua_get("HELPERS.resolved_fg('magit.diff.added')")
  local removed = child.lua_get("HELPERS.resolved_fg('magit.diff.removed')")
  MiniTest.expect.equality(type(added), "number", "added has a foreground")
  MiniTest.expect.equality(type(removed), "number", "removed has a foreground")
  MiniTest.expect.equality(added ~= removed, true, "added and removed differ")

  --- Perceived brightness of a 24-bit colour, for relative-dimness checks.
  ---@param colour integer
  ---@return number
  local function luminance(colour)
    local r = math.floor(colour / 65536) % 256
    local g = math.floor(colour / 256) % 256
    local b = colour % 256
    return 0.2126 * r + 0.7152 * g + 0.0722 * b
  end

  local planned = child.lua_get("HELPERS.resolved_fg('magit.menu.planned')")
  local label = child.lua_get("HELPERS.resolved_fg('magit.menu.label')")
  MiniTest.expect.equality(type(planned), "number", "menu.planned has a foreground")
  MiniTest.expect.equality(type(label), "number", "menu.label has a foreground")
  MiniTest.expect.equality(
    luminance(planned) < luminance(label),
    true,
    "menu.planned must be dimmer than menu.label"
  )

  child.lua("vim.cmd('colorscheme vim')")
  MiniTest.expect.equality(child.lua_get("HELPERS.missing_links()"), {})

  local switched_added = child.lua_get("HELPERS.resolved_fg('magit.diff.added')")
  local switched_removed = child.lua_get("HELPERS.resolved_fg('magit.diff.removed')")
  MiniTest.expect.equality(switched_added ~= switched_removed, true, "added and removed differ")
  MiniTest.expect.equality(switched_added ~= added, true, "the scheme changed the colour")

  local headings = {
    { name = "section", fg = child.lua_get("HELPERS.resolved_fg('magit.section')") },
    { name = "file", fg = child.lua_get("HELPERS.resolved_fg('magit.file')") },
    { name = "hunk header", fg = child.lua_get("HELPERS.resolved_fg('magit.hunk.header')") },
    { name = "menu key", fg = child.lua_get("HELPERS.resolved_fg('magit.menu.key')") },
  }
  for i = 1, #headings do
    MiniTest.expect.equality(
      type(headings[i].fg),
      "number",
      headings[i].name .. " has a foreground"
    )
    for j = 1, i - 1 do
      MiniTest.expect.equality(
        headings[i].fg ~= headings[j].fg,
        true,
        headings[i].name .. " must differ from " .. headings[j].name
      )
    end
  end
end

T["tokens are refreshed after a mutation"] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get("HELPERS.wait_tokens()"), true, "semantic tokens arrive")

  child.lua("M.invoke('status.stage-all')")
  MiniTest.expect.equality(
    child.lua_get("HELPERS.wait_for(function() return HELPERS.has_line('Staged changes (1)') end)"),
    true,
    "the mutation rendered"
  )

  local staged = child.lua_get([[
    (function()
      return vim.wait(5000, function()
        for i, line in ipairs(STATUS.lines) do
          if line == '  M f.txt' then
            local token = HELPERS.token_at(i - 1, 2)
            if token and token.modifiers and token.modifiers.staged then
              return true
            end
          end
        end
        return false
      end, 20)
    end)()
  ]])
  MiniTest.expect.equality(staged, true, "staged tokens after the mutation")
end

T["uses manual folding and serves fold ranges from the server"] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get("vim.wo.foldmethod"), "manual")
  MiniTest.expect.equality(child.lua_get("vim.wo.foldenable"), false)

  local count = child.lua_get([[
    (function()
      local r = vim.lsp.buf_request_sync(STATUS.buf, 'textDocument/foldingRange', {
        textDocument = { uri = STATUS.uri },
      }, 5000)
      for _, res in pairs(r or {}) do
        return #(res.result or {})
      end
      return 0
    end)()
  ]])
  MiniTest.expect.equality(count > 0, true, "expected at least one folding range")
end

T["status buffer is read-only to native edits"] = function()
  setup_child()
  local before = child.lua_get("vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)")

  local function opt(name)
    return child.lua_get(
      string.format("vim.api.nvim_get_option_value(%q, { buf = STATUS.buf })", name)
    )
  end
  MiniTest.expect.equality(opt("buftype"), "nofile")
  MiniTest.expect.equality(opt("bufhidden"), "hide")
  MiniTest.expect.equality(opt("swapfile"), false)
  MiniTest.expect.equality(opt("modifiable"), false)
  -- `readonly` must stay unset: a readonly buffer warns W10 on every
  -- programmatic render (bug 4).
  MiniTest.expect.equality(opt("readonly"), false)
  MiniTest.expect.equality(opt("undolevels"), -1)
  MiniTest.expect.equality(opt("modified"), false)

  MiniTest.expect.equality(child.lua_get("HELPERS.wait_idle()"), true)
  -- Non-vacuity: enable modifiable only for the guard's actual native x.
  -- It snapshots the successful edit and restores in a finally-style cleanup
  -- before returning, without yielding to a render or retrying input.
  local guard_ok, guard_err = pcall(type_edit, "x", { native = true, control = true })
  MiniTest.expect.equality(
    guard_ok,
    false,
    { fail_reason = "the guard must reject a successful edit: " .. tostring(guard_err) }
  )
  MiniTest.expect.equality(
    tostring(guard_err):find("unchanged after x", 1, true) ~= nil,
    true,
    { fail_reason = "guard failure must be the buffer-unchanged check: " .. tostring(guard_err) }
  )
  MiniTest.expect.equality(
    child.lua_get("HELPERS.lines()"),
    before,
    { fail_reason = "mutation must be fully restored" }
  )
  MiniTest.expect.equality(opt("modifiable"), false)
  MiniTest.expect.equality(opt("modified"), false)

  -- `u` is bound to unstage and is a no-op with the cursor on the header, so
  -- it is classified as a bound no-op (no E21 expected).
  type_edit("u", { bound_noop = true })
  -- `J` and `~` are unbound in every profile, so they must fail with E21.
  type_edit("J")
  type_edit("~")
  -- `c` opens the commit view, so `ciw<Esc>` is not an unbound edit; type the
  -- native `ciw` with mappings bypassed to assert E21 honestly.
  type_edit("ciw<Esc>", { native = true })

  local ok, err = pcall(child.cmd, "s/a/b/")
  MiniTest.expect.equality(ok, false, { fail_reason = "substitute should be refused" })
  MiniTest.expect.equality(
    tostring(err):find("E21", 1, true) ~= nil,
    true,
    { fail_reason = "substitute should fail with E21: " .. tostring(err) }
  )
  child.v.errmsg = ""

  local after = child.lua_get("vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)")
  MiniTest.expect.equality(after, before)
  MiniTest.expect.equality(opt("modified"), false)
end

T["refresh and mutations still work after blocked edits"] = function()
  setup_child()
  -- `d` now opens the diff menu (Magit's key), so use native edits that are
  -- unbound in every profile.
  type_edit("J")
  type_edit("~")

  local path = repo .. "/f.txt"
  local lines = vim.fn.readfile(path)
  table.insert(lines, "line11")
  vim.fn.writefile(lines, path)

  child.lua("M.refresh()")
  MiniTest.expect.equality(child.lua_get("HELPERS.wait_idle()"), true)
  assert_line("    +line11")
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('modified', { buf = STATUS.buf })"),
    false
  )

  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(invoke_at(target, "status.stage"), true)
  assert_line("Staged changes (1)")
end

T["write is a no-op and keeps the buffer unmodified"] = function()
  setup_child()
  child.lua([[
    HINTS = {}
    vim.notify = function(msg, level)
      table.insert(HINTS, { msg = msg, level = level })
    end
  ]])
  local before = child.lua_get("vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)")

  local ok, err = pcall(child.type_keys, 100, ":write<CR>")
  MiniTest.expect.equality(ok, true, "write must not raise: " .. tostring(err))
  -- Wait until the queued `:write` actually ran (its hint is the observable
  -- result) before reading `v:errmsg` below.
  MiniTest.expect.equality(
    child.lua_get("HELPERS.wait_for(function() return #HINTS >= 1 end, 2000)"),
    true,
    "write must reach the read-only hint"
  )
  MiniTest.expect.equality(child.v.errmsg, "", "write must not produce a Vim error")
  child.v.errmsg = ""

  MiniTest.expect.equality(child.lua_get("#HINTS"), 1)
  local msg = child.lua_get("HINTS[1].msg")
  MiniTest.expect.equality(type(msg), "string")
  MiniTest.expect.equality(msg:find("Read-only view", 1, true) ~= nil, true, msg)
  MiniTest.expect.equality(child.lua_get("HINTS[1].level"), vim.log.levels.INFO)

  child.type_keys(100, ":write<CR>")
  -- The second write must not add a hint; poll for the violation until the
  -- queued `:write` has had time to run, so absence is a real observation.
  MiniTest.expect.equality(
    child.lua_get("HELPERS.wait_for(function() return #HINTS > 1 end, 500)"),
    false,
    "the hint is shown once per session"
  )

  local after = child.lua_get("vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)")
  MiniTest.expect.equality(after, before)
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('modified', { buf = STATUS.buf })"),
    false
  )
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('modifiable', { buf = STATUS.buf })"),
    false
  )
end

T["navigation jumps use server-owned structured headers"] = function()
  setup_child()
  local symbols = child.lua_get([[
    (function()
      local r = vim.lsp.buf_request_sync(STATUS.buf, 'textDocument/documentSymbol', {
        textDocument = { uri = STATUS.uri },
      }, 5000)
      for _, res in pairs(r or {}) do
        return res.result or {}
      end
      return {}
    end)()
  ]])
  MiniTest.expect.equality(#symbols > 0, true, "documentSymbol returns items")

  local staged_line, unstaged_section_line, unstaged_line, hunk_line
  for i, line in ipairs(child.lua_get("STATUS.lines")) do
    if line == "Staged changes (0)" then
      staged_line = i
    elseif line == "Unstaged changes (1)" then
      unstaged_section_line = i
    elseif line == "  M f.txt" then
      unstaged_line = i
    elseif line == "    @@ -1,10 +1,10 @@" then
      hunk_line = i
    end
  end

  -- Symbol navigation is asynchronous: move, then wait for the cursor to land.
  local function jump_at(kind, dir, from, want)
    child.lua(
      string.format(
        "vim.api.nvim_win_set_cursor(0, {%s}); M.jump(%q, %d); "
          .. "J = vim.wait(5000, function() return vim.api.nvim_win_get_cursor(0)[1] == %d end, 10)",
        from,
        kind,
        dir,
        want
      )
    )
    MiniTest.expect.equality(child.lua_get("J"), true, kind .. " jumps to " .. want)
  end

  jump_at("section", 1, "1, 0", staged_line)
  jump_at("file", 1, "1, 0", unstaged_line)
  jump_at("hunk", 1, "1, 0", hunk_line)
  jump_at("section", -1, 'vim.fn.line("$"), 0', unstaged_section_line)
end

T["gO outline lists document symbols"] = function()
  setup_child()
  local items = child.lua_get([[
    (function()
      local captured
      vim.lsp.buf.document_symbol({ on_list = function(t) captured = t.items end })
      vim.wait(5000, function() return captured ~= nil end, 20)
      return captured or {}
    end)()
  ]])
  MiniTest.expect.equality(#items > 0, true, "the outline has symbols")
  local has_section = false
  for _, item in ipairs(items) do
    if item.text:find("Unstaged changes", 1, true) then
      has_section = true
    end
  end
  MiniTest.expect.equality(has_section, true)
end

T["<CR> on a change line opens the real file at the new-side line"] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'line3 CHANGED', 'unstaged')")
  MiniTest.expect.equality(type(target), "number", "change line index")
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, "<CR>")

  local opened = child.lua_get(
    "HELPERS.wait_for(function() return vim.api.nvim_buf_get_name(0):match('f%.txt$') ~= nil end)"
  )
  MiniTest.expect.equality(opened, true, "the worktree file opened")
  MiniTest.expect.equality(child.lua_get('vim.fn.line(".")'), 3)
end

T["rapid successive invokes keep the buffer consistent with the server"] = function()
  setup_child()
  child.lua([[
    for _ = 1, 3 do
      M.invoke('status.stage-all')
      M.invoke('status.unstage-all')
    end
    M.invoke('status.stage-all')
    HELPERS.wait_idle()
  ]])

  local buffer = child.lua_get("HELPERS.lines()")
  local resync = child.lua_get("HELPERS.resync()")
  MiniTest.expect.equality(buffer, resync.text)
  MiniTest.expect.equality(child.lua_get("STATUS.version"), resync.version)
  assert_line("Staged changes (1)")
end

T["a lost update triggers a full resync"] = function()
  setup_child()
  -- Pretend the client missed a render: the next edits no longer match.
  child.lua("STATUS.version = STATUS.version - 1")
  child.lua("M.invoke('status.stage-all'); HELPERS.wait_idle()")

  local buffer = child.lua_get("HELPERS.lines()")
  local resync = child.lua_get("HELPERS.resync()")
  MiniTest.expect.equality(buffer, resync.text)
  MiniTest.expect.equality(child.lua_get("STATUS.version"), resync.version)
  assert_line("Staged changes (1)")
end

T["buffer-local maps match the server keymap (vim profile)"] = function()
  setup_child()
  for _, mode in ipairs({ "n", "x" }) do
    local diff = child.lua_get(string.format("HELPERS.keymap_diff(%q)", mode))
    MiniTest.expect.equality(diff.missing, {}, "missing maps in mode " .. mode)
    MiniTest.expect.equality(diff.extra, {}, "stale maps in mode " .. mode)
  end
end

T["buffer-local maps match the server keymap (standard profile)"] = function()
  setup_child({ profile = "standard" })
  for _, mode in ipairs({ "n", "x" }) do
    local diff = child.lua_get(string.format("HELPERS.keymap_diff(%q)", mode))
    MiniTest.expect.equality(diff.missing, {}, "missing maps in mode " .. mode)
    MiniTest.expect.equality(diff.extra, {}, "stale maps in mode " .. mode)
  end
end

T["a second view/keymap replaces the applied maps"] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['u'] ~= nil"), true)
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['gr'] ~= nil"), true)

  child.lua([[
    VIEW.apply_keymap({
      viewId = STATUS.id,
      scope = 'status',
      version = STATUS.version,
      capture = false,
      bindings = {
        {
          command = 'status.stage',
          keys = { 's' },
          modes = { 'normal', 'visual' },
          label = 'Stage',
          doc = '',
          group = 'status',
          status = 'implemented',
        },
      },
    }, STATUS.client_id)
  ]])

  local normal = child.lua_get("HELPERS.applied_lhs('n')")
  MiniTest.expect.equality(normal["u"], nil, "the old unstage map is gone")
  MiniTest.expect.equality(normal["gr"], nil, "the old chord is gone")
  MiniTest.expect.equality(normal["s"] ~= nil, true, "the new map is applied")
end

T["pressing s stages the change under the cursor"] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), "number", "change line index")
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, "s")

  local staged =
    child.lua_get("HELPERS.wait_for(function() return HELPERS.has_line('Staged changes (1)') end)")
  MiniTest.expect.equality(staged, true)
  MiniTest.expect.equality(
    git_out({ "diff", "--cached", "-U3", "--", "f.txt" }):find("+NEW line", 1, true) ~= nil,
    true
  )
end

T["the g r chord refreshes the view"] = function()
  setup_child()
  local path = repo .. "/f.txt"
  local lines = vim.fn.readfile(path)
  table.insert(lines, "line11")
  vim.fn.writefile(lines, path)

  child.type_keys(50, "g", "r")
  local refreshed =
    child.lua_get("HELPERS.wait_for(function() return HELPERS.has_line('    +line11') end)")
  MiniTest.expect.equality(refreshed, true)
end

T["Tab toggles a file section through the server"] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  MiniTest.expect.equality(type(target), "number", "file line index")
  local before = child.lua_get("vim.api.nvim_buf_line_count(STATUS.buf)")
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, "<Tab>")

  local collapsed = child.lua_get(
    string.format(
      "HELPERS.wait_for(function() return vim.api.nvim_buf_line_count(STATUS.buf) < %d end)",
      before
    )
  )
  MiniTest.expect.equality(collapsed, true, "the file body collapsed")
  assert_no_line("    +NEW line")
end

T["an unbound key passes through natively"] = function()
  setup_child()
  child.api.nvim_win_set_cursor(0, { 1, 1 })
  child.type_keys(50, "h")
  MiniTest.expect.equality(child.lua_get('vim.fn.col(".")'), 1)
end

T["Escape with a selection passes through and leaves visual mode"] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), "number", "change line index")
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, "V", "<Esc>")
  MiniTest.expect.equality(child.lua_get("vim.fn.mode()"), "n")
end

T["mirrors a user mapping whose rhs is a fold command"] = function()
  child.cmd("nnoremap <F2> za")
  setup_child()
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['<F2>'] ~= nil"), true)

  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  local before = child.lua_get("vim.api.nvim_buf_line_count(STATUS.buf)")
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, "<F2>")

  local collapsed = child.lua_get(
    string.format(
      "HELPERS.wait_for(function() return vim.api.nvim_buf_line_count(STATUS.buf) < %d end)",
      before
    )
  )
  MiniTest.expect.equality(collapsed, true, "the mirrored fold key collapsed the file")
end

T["a user Tab fold mapping toggles the section"] = function()
  child.cmd("nnoremap <Tab> za")
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  local before = child.lua_get("vim.api.nvim_buf_line_count(STATUS.buf)")
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, "<Tab>")

  local collapsed = child.lua_get(
    string.format(
      "HELPERS.wait_for(function() return vim.api.nvim_buf_line_count(STATUS.buf) < %d end)",
      before
    )
  )
  MiniTest.expect.equality(collapsed, true, "the user Tab mapping toggled the file")
end

T["fold_keys mirrors a Lua-function fold mapping"] = function()
  setup_child({ fold_keys = { ["<F3>"] = "common.toggle-section" } })
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['<F3>'] ~= nil"), true)

  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  local before = child.lua_get("vim.api.nvim_buf_line_count(STATUS.buf)")
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, "<F3>")

  local collapsed = child.lua_get(
    string.format(
      "HELPERS.wait_for(function() return vim.api.nvim_buf_line_count(STATUS.buf) < %d end)",
      before
    )
  )
  MiniTest.expect.equality(collapsed, true, "the configured fold key collapsed the file")
end

T["capture mode forwards printable keys and shows the server hint"] = function()
  setup_child()
  child.lua([[
    VIEW.apply_keymap({
      viewId = STATUS.id,
      scope = 'menu',
      version = STATUS.version,
      capture = true,
      bindings = {},
    }, STATUS.client_id)
    SENT = {}
    local lsp_client = vim.lsp.get_client_by_id(STATUS.client_id)
    local original_request = lsp_client.request
    lsp_client.request = function(self, method, params, handler, bufnr)
      if method == 'view/key' then
        SENT[#SENT + 1] = params.key
        handler(nil, {
          effects = {
            { type = 'message', level = 3, text = 'Unbound key: ' .. params.key },
          },
        })
        return true
      end
      return original_request(self, method, params, handler, bufnr)
    end
    NOTIFIES = {}
    vim.notify = function(msg, level)
      table.insert(NOTIFIES, { msg = msg, level = level })
    end
  ]])

  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['Q'] ~= nil"), true)
  local before = child.lua_get("vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)")
  child.type_keys(50, "Q")
  MiniTest.expect.equality(
    child.lua_get("HELPERS.wait_for(function() return #NOTIFIES == 1 end)"),
    true,
    "the unbound key was reported"
  )

  MiniTest.expect.equality(child.lua_get("SENT"), { "Q" })
  MiniTest.expect.equality(child.lua_get("#NOTIFIES"), 1)
  local msg = child.lua_get("NOTIFIES[1].msg")
  MiniTest.expect.equality(msg:find("Unbound key: Q", 1, true) ~= nil, true, msg)
  local after = child.lua_get("vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)")
  MiniTest.expect.equality(after, before)
end

T["Enter visits the file and new-side line from the server effect"] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'line3 CHANGED', 'unstaged')")
  MiniTest.expect.equality(type(target), "number", "change line index")
  child.api.nvim_win_set_cursor(0, { target, 0 })

  -- `common.visit` is a server command now: the effect names the file and line,
  -- and the client only opens it. No rendered text is parsed.
  child.type_keys(50, "<CR>")
  local opened = child.lua_get(
    "HELPERS.wait_for(function() return vim.api.nvim_buf_get_name(0):match('f%.txt$') ~= nil end)"
  )
  MiniTest.expect.equality(opened, true, "Enter opened the visited file")
  MiniTest.expect.equality(child.lua_get('vim.fn.line(".")'), 3)
  MiniTest.expect.equality(
    child.lua_get("#vim.api.nvim_list_wins()"),
    1,
    "the file opens in the only window"
  )
end

T["ctrl+t makes the buffer editable and ctrl+t restores the server text"] = function()
  setup_child()
  local before = child.lua_get("STATUS.lines")
  local undo = child.lua_get("require('magit-lsp.buffer').undo_levels(STATUS.buf)")
  MiniTest.expect.equality(undo ~= -1, true, "the user undo setting was captured")

  local function opt(name)
    return child.lua_get(
      string.format("vim.api.nvim_get_option_value(%q, { buf = STATUS.buf })", name)
    )
  end
  local function mode_is(wanted)
    return vim.wait(5000, function()
      return child.lua_get("STATUS.mode") == wanted
    end, 20)
  end

  child.type_keys(50, "<C-t>")
  MiniTest.expect.equality(mode_is("text"), true, "ctrl+t entered text mode")
  MiniTest.expect.equality(opt("modifiable"), true)
  MiniTest.expect.equality(opt("undolevels"), undo)
  MiniTest.expect.equality(
    child.lua_get("#vim.api.nvim_buf_get_keymap(STATUS.buf, 'n')"),
    1,
    "only the toggle stays mapped in text mode"
  )
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['s'] == nil"), true)

  child.type_keys(50, "dd")
  child.type_keys(50, "ihello<Esc>")
  -- Wait for the queued edits to land in the buffer instead of a fixed delay.
  MiniTest.expect.equality(
    child.lua_get(
      "HELPERS.wait_for(function() return vim.deep_equal(HELPERS.lines(), STATUS.lines) == false end)"
    ),
    true,
    "the buffer is editable"
  )

  child.type_keys(50, "<C-t>")
  MiniTest.expect.equality(mode_is("interactive"), true, "ctrl+t left text mode")
  vim.wait(5000, function()
    return child.lua_get("STATUS.lines") ~= nil
  end, 20)
  MiniTest.expect.equality(child.lua_get("HELPERS.lines()"), before, "the server text is back")
  MiniTest.expect.equality(opt("modifiable"), false)
  MiniTest.expect.equality(opt("readonly"), false)
  MiniTest.expect.equality(opt("undolevels"), -1)
  MiniTest.expect.equality(opt("modified"), false)

  child.lua('pcall(vim.cmd, "silent! undo")')
  MiniTest.expect.equality(
    child.lua_get("HELPERS.lines()"),
    before,
    "undo cannot restore the stale text-mode edits"
  )
end

T["y y copies the value at point to the clipboard"] = function()
  -- A provider keeps `+` deterministic in headless runs.
  child.lua([[
    CLIP = nil
    vim.g.clipboard = {
      name = 'test',
      copy = {
        ['+'] = function(lines) CLIP = lines end,
        ['*'] = function(lines) CLIP = lines end,
      },
      paste = {
        ['+'] = function() return CLIP or { '' } end,
        ['*'] = function() return CLIP or { '' } end,
      },
    }
  ]])
  setup_child()

  local function clear()
    child.lua([[vim.fn.setreg('+', ''); vim.fn.setreg('"', '')]])
  end
  local function copied()
    return vim.wait(5000, function()
      return child.lua_get([[vim.fn.getreg('+')]]) ~= ""
    end, 20)
  end

  clear()
  local added = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(added), "number", "change line index")
  child.api.nvim_win_set_cursor(0, { added, 0 })
  child.type_keys(50, "yy")
  MiniTest.expect.equality(copied(), true, "the line was copied")
  MiniTest.expect.equality(child.lua_get([[vim.fn.getreg('+')]]), "NEW line")
  MiniTest.expect.equality(child.lua_get([[vim.fn.getreg('"')]]), "NEW line")

  clear()
  local hunk = child.lua_get("HELPERS.find_first('hunk', 'unstaged')")
  child.api.nvim_win_set_cursor(0, { hunk, 0 })
  child.type_keys(50, "yy")
  MiniTest.expect.equality(copied(), true, "the hunk was copied")
  local hunk_text = child.lua_get([[vim.fn.getreg('+')]])
  MiniTest.expect.equality(hunk_text:find("line1\n", 1, true), 1, hunk_text)
  MiniTest.expect.equality(hunk_text:find("\nNEW line", 1, true) ~= nil, true, hunk_text)

  clear()
  local file = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  child.api.nvim_win_set_cursor(0, { file, 0 })
  child.type_keys(50, "yy")
  MiniTest.expect.equality(copied(), true, "the path was copied")
  MiniTest.expect.equality(child.lua_get([[vim.fn.getreg('+')]]), "f.txt")
end

local wait_menu = dofile("tests/magit_lsp/menu.lua")(T, child, setup_child)

T["debug.snapshot reports the status view state"] = function()
  setup_child()
  local encoded = child.lua_get([[
    (function()
      local debug_api = require('magit-lsp.debug')
      vim.wait(5000, function()
        return debug_api.idle()
      end, 10)
      return vim.json.encode(debug_api.snapshot())
    end)()
  ]])
  local snapshot = vim.json.decode(encoded)

  MiniTest.expect.equality(snapshot.mode, "n")
  MiniTest.expect.equality(type(snapshot.cursor.row), "number")
  MiniTest.expect.equality(type(snapshot.cursor.col), "number")
  MiniTest.expect.equality(snapshot.idle, true)
  MiniTest.expect.equality(snapshot.pending, "")

  MiniTest.expect.equality(#snapshot.view_stack, 1)
  MiniTest.expect.equality(snapshot.view_stack[1].id, child.lua_get("STATUS.key"))
  MiniTest.expect.equality(snapshot.view_stack[1].kind, "status")

  MiniTest.expect.equality(snapshot.render.version, snapshot.render.acked)
  MiniTest.expect.equality(snapshot.render.version > 0, true, "a version was rendered and acked")

  MiniTest.expect.equality(snapshot.keymap.scope, "status")
  MiniTest.expect.equality(snapshot.keymap.capture, false)
  local refresh_key = child.lua_get([[
    (function()
      for _, binding in ipairs(STATUS.keymap.bindings or {}) do
        if binding.command == 'common.refresh' and binding.keys[1] then
          return binding.keys[1]
        end
      end
      return nil
    end)()
  ]])
  MiniTest.expect.equality(vim.tbl_contains(snapshot.keymap.keys, refresh_key), true)

  local status_buf = child.lua_get("STATUS.buf")
  local status_window
  for _, window in ipairs(snapshot.windows) do
    if window.buffer == status_buf then
      status_window = window
    end
  end
  MiniTest.expect.equality(type(status_window), "table", "the status window is reported")
  MiniTest.expect.equality(status_window.float, false)
  MiniTest.expect.equality(status_window.filetype, "magit-lsp")
  MiniTest.expect.equality(status_window.width > 0, true)
  MiniTest.expect.equality(status_window.height > 0, true)

  MiniTest.expect.equality(type(snapshot.messages), "table")
  MiniTest.expect.equality(type(snapshot.notifications), "table")
end

T["buffers renders that beat the view/open response"] = function()
  child.lua(string.format(
    [[
    DELAYED_OPEN = false
    RENDER_BEFORE_RESPONSE = false
    local config = require('magit-lsp.config')
    require('magit-lsp').setup({ root_dir = %q, cmd = function(dispatchers, cfg)
      local rpc = vim.lsp.rpc.start(config.server_command(), dispatchers, {
        cwd = cfg.cmd_cwd, env = cfg.cmd_env, detached = cfg.detached,
      })
      local request = rpc.request
      rpc.request = function(method, params, handler, notify_reply_callback)
        if method == 'view/open' then
          DELAYED_OPEN = true
          return request(method, params, function(err, result, id)
            vim.defer_fn(function()
              RENDER_BEFORE_RESPONSE = next(VIEW.pending_render) ~= nil
              handler(err, result, id)
            end, 300)
          end, notify_reply_callback)
        end
        return request(method, params, handler, notify_reply_callback)
      end
      return rpc
    end })
  ]],
    repo
  ))
  setup_child()
  MiniTest.expect.equality(child.lua_get("DELAYED_OPEN"), true, "the open response was delayed")
  MiniTest.expect.equality(
    child.lua_get("RENDER_BEFORE_RESPONSE"),
    true,
    "the render beat the view/open response"
  )

  assert_line("Head: main")
  assert_line("Unstaged changes (1)")
  assert_line("  M f.txt")
  assert_line("    +NEW line")

  local diff = child.lua_get("HELPERS.keymap_diff()")
  MiniTest.expect.equality(diff.missing, {}, "the early keymap was applied")
  MiniTest.expect.equality(diff.extra, {}, "the early keymap was applied")

  local state = child.lua_get([[
    (function()
      local snapshot = require('magit-lsp.debug').snapshot()
      return {
        version = snapshot.render.version,
        acked = snapshot.render.acked,
        scope = snapshot.keymap.scope,
      }
    end)()
  ]])
  MiniTest.expect.equality(state.version, state.acked)
  MiniTest.expect.equality(state.version > 0, true, "the buffered render was applied")
  MiniTest.expect.equality(state.scope, "status")
end

T["a normal setup never loads the debug module"] = function()
  child.restart({ "-u", "NONE" })
  child.lua(string.format(
    [[
    vim.opt.rtp:append(%q)
    M = require('magit-lsp')
    M.setup({ root_dir = %q })
    vim.wait(5000, function()
      local v = M.status_view()
      return v ~= nil and v.version ~= nil
    end, 10)
  ]],
    project,
    repo
  ))

  MiniTest.expect.equality(
    child.lua_get("package.loaded['magit-lsp.debug'] == nil"),
    true,
    "a normal setup must not load the debug recorder"
  )
end

T["debug.trace writes one JSON line per event"] = function()
  local trace_path = project .. "/build/test-output/trace-lua.jsonl"
  vim.fn.mkdir(project .. "/build/test-output", "p")
  vim.fn.delete(trace_path)
  child.restart({
    "-u",
    "scripts/minimal_init.lua",
    "--cmd",
    string.format('let $MAGIT_LSP_TRACE="%s"', trace_path),
  })
  setup_child()

  child.lua([[
    local debug_api = require('magit-lsp.debug')
    local before = STATUS.version
    M.press({ 'g', 'r' })
    vim.wait(5000, function()
      return STATUS.version ~= before and debug_api.idle()
    end, 10)
    debug_api.idle()
  ]])

  local lines = vim.fn.readfile(trace_path)
  MiniTest.expect.equality(#lines > 0, true, "the trace file has events")
  local seen = {}
  local times = {}
  for _, line in ipairs(lines) do
    local ok, decoded = pcall(vim.json.decode, line)
    MiniTest.expect.equality(ok, true, "a trace line is JSON: " .. line)
    MiniTest.expect.equality(type(decoded.t_ms), "number", "t_ms is a number")
    MiniTest.expect.equality(type(decoded.id), "number", "id is a number")
    MiniTest.expect.equality(type(decoded.event), "string", "event is a string")
    MiniTest.expect.equality(decoded.detail ~= nil, true, "detail is present")
    seen[decoded.event] = true
    times[#times + 1] = decoded.t_ms
  end
  for _, name in ipairs({
    "key_pressed",
    "key_sent",
    "render_received",
    "render_applied",
    "window_opened",
    "idle",
  }) do
    MiniTest.expect.equality(seen[name], true, name .. " was traced")
  end
  for index = 2, #times do
    MiniTest.expect.equality(times[index] >= times[index - 1], true, "the clock is monotonic")
  end
end

T["two repositories get independent status views"] = function()
  setup_child()
  local other = make_repo(true, project .. "/build/test-repos/fixture-other")
  vim.fn.writefile({ "other" }, other .. "/other.txt")

  child.lua(
    string.format(
      "vim.cmd('tabnew'); M.setup({ root_dir = %q }); "
        .. "vim.wait(5000, function() local v = M.status_view(%q); "
        .. "return v ~= nil and v.version ~= nil and v.keymap_ready end, 10)",
      other,
      other
    )
  )
  local state = child.lua_get([==[(function()
    local v = require('magit-lsp.view')
    local out = {}
    for _, id in ipairs(v.order) do
      local record = v.views[id]
      if record.kind == 'status' then
        out[#out + 1] = {
          id = record.id,
          key = record.key,
          client_id = record.client_id,
          buf = record.buf,
          root = record.root,
          lines = vim.api.nvim_buf_get_lines(record.buf, 0, -1, false),
        }
      end
    end
    return out
  end)()]==])

  MiniTest.expect.equality(#state, 2, "both repositories opened a status view")
  local first, second = state[1], state[2]
  MiniTest.expect.equality(first.root, repo)
  MiniTest.expect.equality(second.root, other)
  MiniTest.expect.equality(first.id, second.id, "independent servers reuse bare view ids")
  MiniTest.expect.equality(first.client_id ~= second.client_id, true)
  MiniTest.expect.equality(first.key ~= second.key, true, "client/view identities are independent")
  MiniTest.expect.equality(first.buf ~= second.buf, true, "buffers are independent")
  MiniTest.expect.equality(vim.tbl_contains(first.lines, "  ? other.txt"), false)
  MiniTest.expect.equality(vim.tbl_contains(second.lines, "  ? other.txt"), true)

  -- A render is routed by view id: applying one for the first view must never
  -- rewrite the second buffer.
  child.lua(string.format(
    [[
    local v = require('magit-lsp.view')
    local record = v.views[%q]
    v.apply_render({ viewId = %q, version = (record.version or 0) + 1, text = { 'FIRST ONLY' } }, record.client_id)
  ]],
    first.key,
    first.id
  ))
  MiniTest.expect.equality(
    child.lua_get(string.format("vim.api.nvim_buf_get_lines(%d, 0, -1, false)", first.buf)),
    { "FIRST ONLY" }
  )
  MiniTest.expect.equality(
    child.lua_get(
      string.format(
        'vim.tbl_contains(vim.api.nvim_buf_get_lines(%d, 0, -1, false), "FIRST ONLY")',
        second.buf
      )
    ),
    false,
    "the second view kept its own text"
  )

  -- Quitting a nested view must focus the existing status record, preserving
  -- its buffer, cursor and folds. The other repository stays live throughout.
  child.lua(string.format(
    [[
    VIEW.focus(%q)
    local status_get = vim.lsp.get_client_by_id(STATUS.client_id)
    local render
    local done = false
    status_get:request('view/resync', { viewId = STATUS.id }, function(_, value)
      render = value
      done = true
    end, STATUS.buf)
    vim.wait(5000, function() return done end, 10)
    VIEW.apply_render(render, STATUS.client_id)
    local parent = VIEW.views[%q]
    vim.api.nvim_win_set_cursor(parent.win, { 8, 2 })
    vim.api.nvim_win_call(parent.win, function() vim.cmd('2,4fold') end)
    PARENT = { key = parent.key, buf = parent.buf, version = parent.version,
      cursor = vim.api.nvim_win_get_cursor(parent.win), fold = vim.fn.foldclosed(3),
      text = parent.lines }
    M.press({ 'g', '$' })
    vim.wait(5000, function()
      local current = VIEW.current()
      return current ~= nil and current.key ~= %q
        and current.version ~= nil and current.keymap_ready
    end, 10)
    NESTED = VIEW.current()
    M.press({ 'q' })
    vim.wait(5000, function()
      local current = VIEW.current()
      return current ~= nil and current.key == %q
    end, 10)
    RETURNED = VIEW.current()
    RETURNED_STATE = { key = RETURNED.key, buf = RETURNED.buf, version = RETURNED.version,
      cursor = vim.api.nvim_win_get_cursor(RETURNED.win), fold = vim.fn.foldclosed(3),
      text = RETURNED.lines }
  ]],
    first.key,
    first.key,
    first.key,
    first.key
  ))
  MiniTest.expect.equality(child.lua_get("NESTED.placement"), "main")
  MiniTest.expect.equality(child.lua_get("RETURNED == STATUS"), true)
  MiniTest.expect.equality(child.lua_get("RETURNED_STATE"), child.lua_get("PARENT"))
  MiniTest.expect.equality(
    child.lua_get("vim.lsp.buf_is_attached(RETURNED.buf, RETURNED.client_id)"),
    true
  )
  MiniTest.expect.equality(child.lua_get(string.format("VIEW.views[%q] ~= nil", second.key)), true)
  child.lua([[
    FRESH_BEFORE = RETURNED.version
    M.press({ 'g', 'r' })
    FRESH_REFRESHED = vim.wait(5000, function() return RETURNED.version > FRESH_BEFORE end, 10)
    M.press({ 'c' })
  ]])
  MiniTest.expect.equality(child.lua_get("FRESH_REFRESHED"), true, "original status can refresh")
  MiniTest.expect.equality(type(wait_menu()), "table", "original status can open a menu")
  child.lua("M.press({ 'q' })")
  MiniTest.expect.equality(
    child.lua_get(
      "HELPERS.wait_for(function() local c = VIEW.current(); return c ~= nil and c.key == STATUS.key end)"
    ),
    true,
    "closing the menu returns to the status view"
  )
  child.lua(string.format(
    [[
    VIEW.focus(%q)
    SECOND_STATUS = VIEW.current()
    SECOND_BEFORE = SECOND_STATUS.version
    M.press({ 'g', 'r' })
    SECOND_REFRESHED = vim.wait(5000, function() return SECOND_STATUS.version > SECOND_BEFORE end, 10)
  ]],
    second.key
  ))
  MiniTest.expect.equality(
    child.lua_get("SECOND_REFRESHED"),
    true,
    "other repository still refreshes"
  )
end

T["commit files own full sync, exact bytes and native saves without auto attachment"] = function()
  setup_child()
  child.lua([[
    EDIT = require('magit-lsp.edit')
    SYNCS, BARRIERS, ANSWERS = {}, {}, {}
    local client = vim.lsp.get_client_by_id(STATUS.client_id)
    local original = client.notify
    client.notify = function(self, method, payload)
      if payload.textDocument and payload.textDocument.uri:match('/COMMIT_EDITMSG$') then
        SYNCS[#SYNCS + 1] = { method, vim.deepcopy(payload) }
        return true
      end
      return original(self, method, payload)
    end
    VIEW.request = function(_, _, method, payload, callback)
      BARRIERS[#BARRIERS + 1] = { method = method, payload = payload, callback = callback }
    end
    VIEW.invoke = function(command, opts)
      ANSWERS[#ANSWERS + 1] = { command, opts.args.runId }
    end
    OPEN_DRAFT = function(id, value)
      VIEW.apply_keymap({ viewId = id, scope = 'commit-message', mode = 'interactive',
        capture = false, bindings = {
          { command = 'common.confirm', keys = { 'Z Z' }, modes = { 'normal' } },
          { command = 'common.cancel', keys = { 'Z Q' }, modes = { 'normal' } },
        },
      }, STATUS.client_id)
      local path = STATUS.root .. '/' .. id .. '/COMMIT_EDITMSG'
      vim.fn.mkdir(vim.fn.fnamemodify(path, ':h'), 'p')
      local file = assert(io.open(path, 'wb')); file:write(value); file:close()
      return EDIT.open({ runId = id, uri = vim.uri_from_fname(path),
        text = value, version = 1, mode = 'realFile', kind = 'commit-message' },
        { client_id = STATUS.client_id, root = STATUS.root })
    end
    ACK = function(index, kind)
      local b = BARRIERS[index]
      local p = b.payload.arguments and b.payload.arguments[1] or b.payload
      b.callback({ type = kind, runId = p.runId, uri = p.uri, version = p.version })
    end
    CALLER_WIN, CALLER_BUF = vim.api.nvim_get_current_win(), vim.api.nvim_get_current_buf()
    CALLER_WIDTH, CALLER_HEIGHT = vim.api.nvim_win_get_width(0), vim.api.nvim_win_get_height(0)
    CALLER_COUNT = #vim.api.nvim_list_wins()
    CHECK_WINDOW = function(draft)
      if draft then
        assert(vim.api.nvim_get_current_win() == CALLER_WIN)
        assert(#vim.api.nvim_list_wins() == CALLER_COUNT)
        assert(vim.api.nvim_win_get_width(0) == CALLER_WIDTH)
        assert(vim.api.nvim_win_get_height(0) == CALLER_HEIGHT)
      else
        assert(not vim.api.nvim_buf_is_valid(DOC.buf))
      end
    end
    DOC = OPEN_DRAFT('draft', 'α\n\n')
    CHECK_WINDOW(true)
    DOCUMENT = { name = vim.api.nvim_buf_get_name(DOC.buf),
      buftype = vim.bo[DOC.buf].buftype, filetype = vim.bo[DOC.buf].filetype,
      editable = vim.bo[DOC.buf].modifiable, base = DOC.base, current = VIEW.current() == DOC,
      attached = vim.lsp.buf_is_attached(DOC.buf, DOC.client_id), server_views = #VIEW.order,
      pending_empty = next(VIEW.pending_keymap) == nil,
      eol = vim.bo[DOC.buf].endofline, lines = vim.api.nvim_buf_get_lines(DOC.buf,0,-1,false) }
    vim.api.nvim_buf_set_lines(DOC.buf, 0, -1, false, { 'β', '' })
    vim.bo[DOC.buf].endofline = false
    vim.cmd.write()
    SAVE_PENDING = VIEW.current() == DOC
    SAVE_STAYS = VIEW.current() == DOC and not DOC.busy and not vim.bo[DOC.buf].modified
    vim.api.nvim_buf_set_lines(DOC.buf, 0, -1, false, { 'finish β' })
    EDIT.confirm(DOC.key)
    FINISH_PENDING = VIEW.views[DOC.key] == DOC
    ACK(1, 'confirmed')
    CONFIRMED = VIEW.views[DOC.key] == nil
    CHECK_WINDOW(false)
  ]])
  MiniTest.expect.equality(child.lua_get("DOCUMENT"), {
    name = child.lua_get("STATUS.root") .. "/draft/COMMIT_EDITMSG",
    buftype = "",
    filetype = "gitcommit",
    editable = true,
    base = child.lua_get("STATUS.key"),
    current = true,
    attached = false,
    server_views = 1,
    pending_empty = true,
    eol = true,
    lines = { "α", "" },
  })
  MiniTest.expect.equality(
    child.lua_get("SAVE_PENDING and SAVE_STAYS and FINISH_PENDING and CONFIRMED"),
    true
  )
  MiniTest.expect.equality(child.lua_get("SYNCS"), {
    {
      "textDocument/didOpen",
      {
        textDocument = {
          uri = child.lua_get("DOC.uri"),
          languageId = "gitcommit",
          version = 1,
          text = "α\n\n",
        },
      },
    },
    {
      "textDocument/didChange",
      {
        textDocument = { uri = child.lua_get("DOC.uri"), version = 2 },
        contentChanges = { { text = "β\n" } },
      },
    },
    {
      "textDocument/didChange",
      {
        textDocument = { uri = child.lua_get("DOC.uri"), version = 3 },
        contentChanges = { { text = "finish β" } },
      },
    },
  })
  MiniTest.expect.equality(child.lua_get("BARRIERS[1].payload.arguments[1].source"), "synchronized")
  child.lua([[
    vim.notify = function() end
    BARRIERS = {}
    DOC = OPEN_DRAFT('failed', 'initial')
    vim.api.nvim_buf_set_lines(DOC.buf, 0, -1, false, { 'recover β' })
    EDIT.confirm(DOC.key)
    BARRIERS[1].callback(nil, { message = 'checkpoint rejected' })
    FAILED_SAVE = VIEW.current() == DOC and DOC.sync_error == 'checkpoint rejected'
      and vim.bo[DOC.buf].modifiable and DOC.text == 'recover β' and #BARRIERS == 1
    EDIT.save(DOC.key)
    RETRIED_SAVE = not DOC.sync_error and DOC.saved_text == 'recover β'
      and DOC.saved_version == 3
    EDIT.confirm(DOC.key)
    BARRIERS[2].callback(nil, { message = 'finish write rejected' })
    FAILED_FINISH = VIEW.current() == DOC and DOC.sync_error == 'finish write rejected'
      and vim.bo[DOC.buf].modifiable and DOC.text == 'recover β'
    EDIT.cancel(DOC.key)
    local cancel = BARRIERS[3]
    assert(cancel, 'expected registered cancellation request')
    local cancel_payload = cancel.payload.arguments[1]
    CANCEL_PENDING = VIEW.current() == DOC and cancel.method == 'workspace/executeCommand'
      and cancel.payload.command == 'magit.command' and cancel_payload.command == 'common.cancel'
      and cancel_payload.runId == DOC.id and cancel_payload.uri == DOC.uri
      and cancel_payload.version == DOC.version
    ACK(3, 'aborted')
    CHECK_WINDOW(false)

    BARRIERS, SYNCS = {}, {}
    DOC = OPEN_DRAFT('rejected-sync', 'before')
    vim.api.nvim_buf_set_lines(DOC.buf, 0, -1, false, { 'newer local text' })
    EDIT.flush(DOC)
    local version = DOC.version
    local effect = { mode = 'realFile', runId = DOC.id, uri = DOC.uri, version = version,
      kind = 'commit-message', text = 'server snapshot' }
    EDIT.open(effect, VIEW.views[DOC.base])
    BLOCKED_FINISH = VIEW.current() == DOC and DOC.text == 'newer local text'
      and DOC.version == version and vim.bo[DOC.buf].modifiable
    vim.api.nvim_buf_delete(DOC.buf, { force = true })
    AMBIGUOUS_CANCEL = SYNCS[#SYNCS][1] == 'textDocument/didClose'
    EDIT.open(effect, VIEW.views[DOC.base])
    local closed, reopened = SYNCS[#SYNCS - 1], SYNCS[#SYNCS]
    REOPENED_AUTHORITY = closed[1] == 'textDocument/didClose'
      and closed[2].textDocument.uri == DOC.uri
      and reopened[1] == 'textDocument/didOpen'
      and reopened[2].textDocument.uri == DOC.uri
      and reopened[2].textDocument.version == version
      and reopened[2].textDocument.text == 'newer local text'
    BLOCKED_FINISH = BLOCKED_FINISH and vim.api.nvim_buf_is_valid(DOC.buf)
      and DOC.text == 'newer local text' and DOC.version == version
      and vim.bo[DOC.buf].modifiable and DOC.sync_error ~= nil
    EDIT.cancel(DOC.key)
    REOPENED_AUTHORITY = REOPENED_AUTHORITY
      and BARRIERS[1].payload.arguments[1].version == version
    ACK(1, 'aborted')
    BLOCKED_FINISH = BLOCKED_FINISH and EDIT.open(effect, VIEW.views[DOC.base]) == nil
  ]])
  child.lua([[CHECK_WINDOW(false)]])
  child.lua([[
    DOC = OPEN_DRAFT('disconnect', 'recover disconnected')
    EDIT.disconnected(DOC.client_id)
    local count = #BARRIERS
    EDIT.confirm(DOC.key)
    DISCONNECTED = VIEW.current() == DOC and DOC.disconnected and vim.bo[DOC.buf].modifiable
      and vim.bo[DOC.buf].modified and #BARRIERS == count
  ]])
  MiniTest.expect.equality(
    child.lua_get("FAILED_SAVE and RETRIED_SAVE and FAILED_FINISH and CANCEL_PENDING"),
    true
  )
  MiniTest.expect.equality(
    child.lua_get("BLOCKED_FINISH and AMBIGUOUS_CANCEL and REOPENED_AUTHORITY"),
    true
  )
  MiniTest.expect.equality(child.lua_get("DISCONNECTED"), true)
end

T["native commit buffer close refuses dirty text and recovers a rejected saved close"] = function()
  setup_child()
  git_out({ "add", "f.txt" })
  child.lua([[
    EDIT = require('magit-lsp.edit')
    VIEW.invoke('commit.create', {}, STATUS.key)
    assert(vim.wait(5000, function() return VIEW.current().document == true end, 10))
    DOC = VIEW.current()
    assert(vim.bo[DOC.buf].bufhidden == 'wipe')
    assert(vim.fn.maparg('<CR>', 'c') == '')
    vim.api.nvim_buf_set_lines(DOC.buf, 0, -1, false, { 'native checkpoint' })
    local ok, err = pcall(vim.cmd, 'bdelete')
    assert(not ok and err:match('E89'), err)
    assert(VIEW.current() == DOC and vim.bo[DOC.buf].modified)
    vim.cmd.write()
    PATH = vim.uri_to_fname(DOC.uri)
    assert(vim.uv.fs_rename(PATH, PATH .. '.original'))
    assert(vim.uv.fs_symlink(PATH .. '.original', PATH))
    vim.api.nvim_buf_set_lines(DOC.buf, 0, -1, false, { 'recover unsaved text' })
    vim.cmd('bdelete!')
    assert(vim.wait(5000, function() return DOC.sync_error ~= nil end, 10),
      vim.inspect({ detached = DOC.detached, valid = vim.api.nvim_buf_is_valid(DOC.buf),
        version = DOC.version, messages = vim.fn.execute('messages') }))
    assert(VIEW.current() == DOC and vim.api.nvim_buf_is_valid(DOC.buf))
    assert(vim.bo[DOC.buf].modifiable and vim.bo[DOC.buf].modified)
    assert(vim.api.nvim_buf_get_lines(DOC.buf, 0, -1, false)[1] == 'recover unsaved text')
    assert(vim.uv.fs_unlink(PATH))
    assert(vim.uv.fs_rename(PATH .. '.original', PATH))
    EDIT.cancel(DOC.key)
    local idle = vim.wait(5000, function() return require('magit-lsp.debug').idle() end, 10)
    assert(idle, vim.inspect({ current = VIEW.current(), doc = DOC,
      messages = vim.fn.execute('messages'), progress = require('magit-lsp.debug').state.progress }))
    assert(vim.api.nvim_get_current_buf() == STATUS.buf)
  ]])
  MiniTest.expect.equality(git_out({ "log", "-1", "--format=%s" }), "init\n")
end

T["descriptor queues, deferred renders and resync preserve client identity and text mode"] = function()
  child.lua([[
    VIEW = require('magit-lsp.view')
    local debug_api = require('magit-lsp.debug')
    ACKS = {}
    RESYNCS = {}
    local clients = {}
    for _, id in ipairs({ 101, 202 }) do
      clients[id] = {
        initialized = true,
        notify = function(_, method, params)
          if method == 'view/rendered' then
            ACKS[#ACKS + 1] = { id, params.viewId, params.version }
          end
        end,
        request = function(_, method, params, callback)
          if method == 'view/close' then
            callback(nil, vim.NIL)
            return true
          end
          assert(method == 'view/resync')
          RESYNCS[#RESYNCS + 1] = { client_id = id, callback = callback }
        end,
      }
    end
    vim.lsp.get_client_by_id = function(id) return clients[id] end
    vim.lsp.buf_is_attached = function() return true end
    vim.lsp.semantic_tokens.force_refresh = function() end
    VIEW.configure({})
    for _, id in ipairs({ 101, 202 }) do
      VIEW.apply_render({ viewId = 'same', version = 1,
        mode = id == 101 and 'text' or 'interactive',
        text = id == 101 and { '  alpha', 'βeta', '' } or { 'second' },
      }, id)
      VIEW.apply_keymap({ viewId = 'same', version = 1, scope = 'new-kind',
        mode = id == 101 and 'text' or 'interactive', capture = false, bindings = {},
      }, id)
    end
    VIEW.apply_render({ viewId = 'same', version = 2, baseVersion = 1, mode = 'text',
      edits = { { startLine = 1, endLine = 2, lines = { '  replacement' } } },
    }, 101)
    A = VIEW.open({ id = 'same', kind = 'new-kind', uri = 'magit://test/a',
      placement = 'main', mode = 'text' }, { client_id = 101 })
    B = VIEW.open({ id = 'same', kind = 'new-kind', uri = 'magit://test/b',
      placement = 'side', mode = 'interactive' }, { client_id = 202 })
    EARLY = {
      a = A.lines, b = B.lines, versions = { A.version, B.version },
      editable = { vim.bo[A.buf].modifiable, vim.bo[B.buf].modifiable },
      keymaps = { A.keymap.scope, B.keymap.scope },
      pending_empty = next(VIEW.pending_render) == nil and next(VIEW.pending_keymap) == nil,
      distinct = A.key ~= B.key and A.buf ~= B.buf and A.win ~= B.win,
    }

    -- A render against a version the client does not hold asks for a resync;
    -- it must not corrupt the text or change the mode.
    VIEW.apply_render({ viewId = 'same', version = 99, baseVersion = 0, mode = 'interactive',
      edits = { { startLine = 0, endLine = 1, lines = { 'WRONG BASE' } } },
    }, 101)
    MISMATCH = { mode = A.mode, editable = vim.bo[A.buf].modifiable,
      text = A.lines, version = A.version, acks = #ACKS,
      resyncs = #RESYNCS, client_id = RESYNCS[1].client_id }
    -- A render that arrives while the resync is outstanding is held, then
    -- applied after the fresh text, so the buffer never mixes versions.
    VIEW.apply_render({ viewId = 'same', version = 5, mode = 'interactive',
      text = { 'held during resync' } }, 101)
    HELD = { text = A.lines, version = A.version,
      deferred = #(VIEW.deferred_render[A.key] or {}) }
    RESYNCS[1].callback(nil, { viewId = 'same', version = 4,
      mode = 'interactive', text = { 'fresh server text' } })
    RECOVERED = { mode = A.mode, editable = vim.bo[A.buf].modifiable,
      text = A.lines, version = A.version, other = B.lines,
      deferred = next(VIEW.deferred_render) == nil }
    VIEW.close(A.key)
    INDEPENDENT_CLOSE = VIEW.views[A.key] == nil and VIEW.views[B.key] == B
    vim.api.nvim_buf_delete(B.buf, { force = true })
    CLEAN = vim.wait(1000, function() return debug_api.idle() end, 10)
      and #VIEW.order == 0 and #debug_api.order == 0
  ]])
  MiniTest.expect.equality(child.lua_get("EARLY"), {
    a = { "  alpha", "  replacement", "" },
    b = { "second" },
    versions = { 2, 1 },
    editable = { true, false },
    keymaps = { "new-kind", "new-kind" },
    pending_empty = true,
    distinct = true,
  })
  MiniTest.expect.equality(child.lua_get("MISMATCH"), {
    mode = "text",
    editable = true,
    text = { "  alpha", "  replacement", "" },
    version = 2,
    acks = 3,
    resyncs = 1,
    client_id = 101,
  })
  MiniTest.expect.equality(child.lua_get("HELD"), {
    text = { "  alpha", "  replacement", "" },
    version = 2,
    deferred = 1,
  })
  MiniTest.expect.equality(child.lua_get("RECOVERED"), {
    mode = "interactive",
    editable = false,
    text = { "held during resync" },
    version = 5,
    other = { "second" },
    deferred = true,
  })
  local acks = child.lua_get("ACKS")
  table.sort(acks, function(a, b)
    return a[1] < b[1] or (a[1] == b[1] and a[3] < b[3])
  end)
  MiniTest.expect.equality(acks, {
    { 101, "same", 1 },
    { 101, "same", 2 },
    { 101, "same", 4 },
    { 101, "same", 5 },
    { 202, "same", 1 },
  })
  MiniTest.expect.equality(child.lua_get("INDEPENDENT_CLOSE and CLEAN"), true)
end

T["shared client contract vectors apply through the Neovim adapter"] = function()
  local fixture =
    vim.json.decode(table.concat(vim.fn.readfile("tests/data/client-contracts.json"), "\n"))
  child.lua(string.format("VECTORS = %s", vim.inspect(fixture.renders)))
  local results = child.lua([[
    VIEW = require('magit-lsp.view')
    ACKS, RESYNCS = {}, {}
    local client = {
      initialized = true,
      notify = function(_, method, params)
        if method == 'view/rendered' then ACKS[#ACKS + 1] = params.version end
      end,
      request = function(_, method, params, callback)
        assert(method == 'view/resync')
        RESYNCS[#RESYNCS + 1] = params.viewId
      end,
    }
    vim.lsp.get_client_by_id = function() return client end
    vim.lsp.buf_is_attached = function() return true end
    vim.lsp.semantic_tokens.force_refresh = function() end
    VIEW.configure({})
    local result = {}
    for _, vector in ipairs(VECTORS) do
      local id = vector.render.viewId
      VIEW.apply_render({ viewId = id, version = vector.initialVersion,
        mode = 'interactive', text = vector.initialLines }, 303)
      local view = VIEW.open({ id = id, kind = 'contract-test', uri = 'magit://test/' .. vector.name,
        placement = 'main', mode = 'interactive' }, { client_id = 303 })
      ACKS, RESYNCS = {}, {}
      VIEW.apply_render(vector.render, 303)
      VIEW.flush_acks(303)
      result[#result + 1] = { lines = vim.api.nvim_buf_get_lines(view.buf, 0, -1, false),
        version = view.version, mode = view.mode, ack = ACKS[#ACKS], resync = RESYNCS[1] }
      VIEW.close(view.key, true)
    end
    return result
  ]])
  for index, vector in ipairs(fixture.renders) do
    local result = results[index]
    MiniTest.expect.equality(result.lines, vector.expectedLines, vector.name .. ": buffer lines")
    MiniTest.expect.equality(
      result.version,
      vector.expectedVersion,
      vector.name .. ": view version"
    )
    if vector.expectedMetadata then
      MiniTest.expect.equality(result.mode, vector.expectedMetadata.mode, vector.name .. ": mode")
    end
    if vector.outcome == "ack" then
      MiniTest.expect.equality(result.ack, vector.render.version, vector.name .. ": ACK")
      MiniTest.expect.equality(result.resync, nil, vector.name .. ": no resync")
    else
      MiniTest.expect.equality(result.ack, nil, vector.name .. ": no ACK")
      MiniTest.expect.equality(result.resync, vector.render.viewId, vector.name .. ": resync")
    end
  end
  local version = require("magit-lsp.version")
  for _, vector in ipairs(fixture.advertisements) do
    local err = version.protocol_error(vector.result, vector.required)
    if vector.accepted then
      MiniTest.expect.equality(err, nil, vector.name .. ": accepted advertisement")
    else
      MiniTest.expect.equality(type(err), "string", vector.name .. ": rejected advertisement")
      for _, fragment in ipairs(vector.errorContains) do
        MiniTest.expect.equality(
          err ~= nil and err:find(fragment, 1, true) ~= nil,
          true,
          vector.name .. ": error contains " .. fragment
        )
      end
    end
  end
end

T["user wipe requests close once per client and server close never echoes"] = function()
  child.lua([[
    local view = require('magit-lsp.view')
    local requests = require('magit-lsp.requests')
    local debug_api = require('magit-lsp.debug')
    CLOSES, WARNINGS = {}, {}
    local clients = {}
    local fail = false
    for _, id in ipairs({ 101, 202 }) do
      clients[id] = {
        initialized = true,
        request = function(_, method, params, callback)
          assert(method == 'view/close')
          CLOSES[#CLOSES + 1] = { id, params.viewId }
          vim.schedule(function()
            callback(fail and { message = 'disconnected' } or nil, vim.NIL)
          end)
          return true
        end,
        notify = function(_, method) assert(method ~= 'view/close', 'close must be a request') end,
      }
    end
    vim.lsp.get_client_by_id = function(id) return clients[id] end
    vim.lsp.buf_is_attached = function() return true end
    vim.notify = function(text) WARNINGS[#WARNINGS + 1] = text end
    view.configure({})
    local function open(id)
      return view.open({ id = 'nested', kind = 'process-log',
        uri = 'magit://test/' .. id, placement = 'side', mode = 'interactive' },
        { client_id = id })
    end
    local other = open(202)
    for _ = 1, 3 do
      local current = open(101)
      vim.api.nvim_buf_delete(current.buf, { force = true })
      assert(vim.wait(1000, function() return requests.pending(101) == 0 end, 10))
      assert(view.views[current.key] == nil and view.views[other.key] == other)
    end
    local server_closed = open(101)
    view.close(server_closed.key, true)
    assert(#CLOSES == 3)
    fail = true
    vim.api.nvim_buf_delete(other.buf, { force = true })
    assert(vim.wait(1000, function() return requests.pending(202) == 0 end, 10))
    local disconnected = open(101)
    clients[101] = nil
    vim.api.nvim_buf_delete(disconnected.buf, { force = true })
    CLEAN_CLOSE = vim.wait(1000, function() return #view.order == 0 and debug_api.idle() end, 10)
  ]])
  MiniTest.expect.equality(child.lua_get("CLOSES"), {
    { 101, "nested" },
    { 101, "nested" },
    { 101, "nested" },
    { 202, "nested" },
  })
  MiniTest.expect.equality(
    child.lua_get("WARNINGS"),
    { "magit-lsp view close failed: disconnected" }
  )
  MiniTest.expect.equality(child.lua_get("CLEAN_CLOSE"), true)
end

return T
