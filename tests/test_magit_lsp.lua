local MiniTest = require('mini.test')
local child = MiniTest.new_child_neovim()

local project = vim.fn.getcwd()
local repo

local function sys(cmd)
  local out = vim.fn.system(cmd)
  if vim.v.shell_error ~= 0 then
    error('command failed: ' .. vim.inspect(cmd) .. '\n' .. out, 2)
  end
  return out
end

--- Run `git` in the fixture repository and return its stdout.
local function git_out(args)
  return sys(vim.list_extend({ 'git', '-C', repo }, args))
end

local function make_repo(with_changes, path)
  if with_changes == nil then
    with_changes = true
  end
  path = path or (project .. '/build/test-repos/fixture')
  sys({ 'rm', '-rf', path })
  vim.fn.mkdir(path, 'p')
  sys({ 'git', 'init', '-q', '-b', 'main', path })
  sys({ 'git', '-C', path, 'config', 'user.email', 't@example.com' })
  sys({ 'git', '-C', path, 'config', 'user.name', 'tester' })

  local lines = {}
  for i = 1, 10 do
    table.insert(lines, 'line' .. i)
  end
  vim.fn.writefile(lines, path .. '/f.txt')

  sys({ 'git', '-C', path, 'add', '.' })
  sys({ 'git', '-C', path, 'commit', '-q', '-m', 'init' })

  if with_changes then
    local changed = {}
    for i = 1, #lines do
      if i == 3 then
        table.insert(changed, 'line3 CHANGED')
      elseif i == 8 then
        -- drop line8
      else
        table.insert(changed, lines[i])
      end
      if i == 5 then
        table.insert(changed, 'NEW line')
      end
    end
    vim.fn.writefile(changed, path .. '/f.txt')
  end

  return path
end

local function setup_child(opts)
  local init = { root_dir = repo }
  for key, value in pairs(opts or {}) do
    init[key] = value
  end
  child.lua(
    string.format("VIEW = require('magit-lsp.view'); M = require('magit-lsp'); M.setup(%s); STATUS = M.status_view()", vim.inspect(init))
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
      ---@return table[]
      symbols = function()
        return M.symbols() or {}
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
      ---@return boolean
      wait_idle = function()
        return HELPERS.wait_for(function() return require('magit-lsp.debug').idle() end)
      end,
    }
  ]])
end

local function assert_line(needle)
  local has = child.lua_get(string.format('HELPERS.has_line(%q)', needle))
  MiniTest.expect.equality(has, true, needle)
end

local function assert_no_line(needle)
  local has = child.lua_get(string.format('HELPERS.has_line(%q)', needle))
  MiniTest.expect.equality(has, false, needle)
end

--- Place the cursor on the first matching rendered line and invoke a command.
local function invoke_at(line, command, opts)
  local call = string.format(
    'vim.api.nvim_win_set_cursor(0, {%d, 0}); R = M.invoke(%q%s)',
    line,
    command,
    opts and (', ' .. opts) or ''
  )
  child.lua(call)
  return child.lua_get('R ~= nil')
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
  child.v.errmsg = ''
  local before = child.lua_get('vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)')
  local ok, err, after, modified, native_errmsg
  if opts.native then
    -- Deliberately bypass mappings to exercise the native command itself;
    -- expand termcodes, then `x` (execute now) + `n` (noremap).
    -- The control edits, snapshots and restores in one child call without
    -- yielding to live renders. Cleanup runs even if input/snapshotting fails.
    local result = child.lua_get(string.format([[
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
    ]], tostring(opts.control == true), keys))
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
  child.v.errmsg = ''
  -- Any command the refused key mapped to (e.g. `u` on the header) must have
  -- drained before the next step, so a pending render cannot leak out.
  if not opts.control then child.lua_get('HELPERS.wait_idle()') end
  -- Independent guards, buffer state first: a silently successful edit (e.g.
  -- the negative control while temporarily modifiable) must be reported as an
  -- unchanged-buffer violation even though no E21 can occur; the E21
  -- classification below then still covers every refusal path, so neither
  -- check is weakened.
  after = after or child.lua_get('vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)')
  if modified == nil then
    modified = child.lua_get("vim.api.nvim_get_option_value('modified', { buf = STATUS.buf })")
  end
  MiniTest.expect.equality(after, before, { fail_reason = 'buffer must be unchanged after ' .. keys })
  MiniTest.expect.equality(
    modified,
    false,
    { fail_reason = 'buffer must stay unmodified after ' .. keys }
  )
  if opts.bound_noop then
    -- A mapped no-op must succeed, not masquerade as a native refusal.
    MiniTest.expect.equality(ok, true, { fail_reason = tostring(err) })
    MiniTest.expect.equality(errmsg, '', { fail_reason = 'mapped no-op must not set an error: ' .. errmsg })
  elseif not ok then
    -- A mapped path must fail natively, never with a Lua traceback.
    MiniTest.expect.equality(
      tostring(err):find('E21', 1, true) ~= nil,
      true,
      { fail_reason = 'unexpected error: ' .. tostring(err) }
    )
  else
    -- An unbound native edit must actually be refused with E21.
    MiniTest.expect.equality(errmsg:find('E21', 1, true) ~= nil, true,
      { fail_reason = 'unexpected error: ' .. errmsg })
  end
end

local T = MiniTest.new_set({
  hooks = {
    pre_once = function()
      sys({ 'cargo', 'build' })
    end,
    pre_case = function()
      repo = make_repo()
      child.restart({ '-u', 'scripts/minimal_init.lua' })
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

T['creates a server-named virtual buffer and attaches LSP'] = function()
  setup_child()
  local buftype = child.lua_get("vim.api.nvim_get_option_value('buftype', { buf = STATUS.buf })")
  MiniTest.expect.equality(buftype, 'nofile')

  local bufname = child.lua_get('vim.api.nvim_buf_get_name(STATUS.buf)')
  MiniTest.expect.equality(bufname, 'magit://status' .. repo)

  local attached = child.lua_get('vim.lsp.buf_is_attached(STATUS.buf, STATUS.client_id)')
  MiniTest.expect.equality(attached, true)
end

T['staging a visual line range stages only those lines'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, 'V', 's')

  local staged = child.lua_get("HELPERS.wait_for(function() return HELPERS.has_line('    +NEW line') end)")
  MiniTest.expect.equality(staged, true)

  local cached = git_out({ 'diff', '--cached', '-U3', '--', 'f.txt' })
  MiniTest.expect.equality(cached:find('+NEW line', 1, true) ~= nil, true, cached)
  MiniTest.expect.equality(cached:find('line3 CHANGED', 1, true), nil, cached)
  MiniTest.expect.equality(git_out({ 'diff', '-U3', '--', 'f.txt' }):find('line3 CHANGED', 1, true) ~= nil, true)
end

T['a failed mutation notifies once with a readable summary'] = function()
  setup_child()
  child.lua([[
    NOTIFIES = {}
    vim.notify = function(msg, level)
      table.insert(NOTIFIES, { msg = msg, level = level })
    end
  ]])
  vim.fn.writefile({}, repo .. '/.git/index.lock')
  child.lua("R = M.invoke('status.stage-all')")
  vim.fn.delete(repo .. '/.git/index.lock')

  MiniTest.expect.equality(child.lua_get('R ~= nil'), true)
  MiniTest.expect.equality(child.lua_get('#NOTIFIES'), 1)
  local msg = child.lua_get('NOTIFIES[1].msg')
  MiniTest.expect.equality(type(msg), 'string')
  MiniTest.expect.equality(
    msg:find('Another Git process is running in this repository.', 1, true) ~= nil,
    true
  )
  MiniTest.expect.equality(msg:find('{', 1, true), nil)
  MiniTest.expect.equality(msg:find('<metatable>', 1, true), nil)
  MiniTest.expect.equality(child.lua_get('NOTIFIES[1].level'), vim.log.levels.ERROR)
end

T['semantic tokens highlight sections, files, hunks and diff lines'] = function()
  setup_child()
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('filetype', { buf = STATUS.buf })"),
    'magit-lsp'
  )
  MiniTest.expect.equality(child.lua_get('HELPERS.wait_tokens()'), true, 'semantic tokens arrive')

  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('Unstaged changes (1)', 0)"),
    'magit.section'
  )
  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('  M f.txt', 2)"),
    'magit.file.modified'
  )
  MiniTest.expect.equality(child.lua_get("HELPERS.token_type_on('  M f.txt', 4)"), 'magit.file')
  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('    @@ -1,10 +1,10 @@', 4)"),
    'magit.hunk.header'
  )
  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('    +line3 CHANGED', 4)"),
    'magit.diff.added'
  )
  MiniTest.expect.equality(
    child.lua_get("HELPERS.token_type_on('    -line3', 4)"),
    'magit.diff.removed'
  )
end

T['every token type and modifier in the legend links to a theme group'] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get('HELPERS.wait_tokens()'), true, 'semantic tokens arrive')
  MiniTest.expect.equality(child.lua_get('HELPERS.missing_links()'), {})
end

T['theme links survive a colorscheme switch'] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get('HELPERS.wait_tokens()'), true, 'semantic tokens arrive')

  local added = child.lua_get("HELPERS.resolved_fg('magit.diff.added')")
  local removed = child.lua_get("HELPERS.resolved_fg('magit.diff.removed')")
  MiniTest.expect.equality(type(added), 'number', 'added has a foreground')
  MiniTest.expect.equality(type(removed), 'number', 'removed has a foreground')
  MiniTest.expect.equality(added ~= removed, true, 'added and removed differ')

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
  MiniTest.expect.equality(type(planned), 'number', 'menu.planned has a foreground')
  MiniTest.expect.equality(type(label), 'number', 'menu.label has a foreground')
  MiniTest.expect.equality(
    luminance(planned) < luminance(label),
    true,
    'menu.planned must be dimmer than menu.label'
  )

  child.lua("vim.cmd('colorscheme vim')")
  MiniTest.expect.equality(child.lua_get('HELPERS.missing_links()'), {})

  local switched_added = child.lua_get("HELPERS.resolved_fg('magit.diff.added')")
  local switched_removed = child.lua_get("HELPERS.resolved_fg('magit.diff.removed')")
  MiniTest.expect.equality(switched_added ~= switched_removed, true, 'added and removed differ')
  MiniTest.expect.equality(switched_added ~= added, true, 'the scheme changed the colour')

  local headings = {
    { name = 'section', fg = child.lua_get("HELPERS.resolved_fg('magit.section')") },
    { name = 'file', fg = child.lua_get("HELPERS.resolved_fg('magit.file')") },
    { name = 'hunk header', fg = child.lua_get("HELPERS.resolved_fg('magit.hunk.header')") },
    { name = 'hash', fg = child.lua_get("HELPERS.resolved_fg('magit.hash')") },
  }
  for i = 1, #headings do
    MiniTest.expect.equality(type(headings[i].fg), 'number', headings[i].name .. ' has a foreground')
    for j = 1, i - 1 do
      MiniTest.expect.equality(
        headings[i].fg ~= headings[j].fg,
        true,
        headings[i].name .. ' must differ from ' .. headings[j].name
      )
    end
  end
end

T['tokens are refreshed after a mutation'] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get('HELPERS.wait_tokens()'), true, 'semantic tokens arrive')

  child.lua("M.invoke('status.stage-all')")
  assert_line('Staged changes (1)')

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
  MiniTest.expect.equality(staged, true, 'staged tokens after the mutation')
end

T['uses manual folding and serves fold ranges from the server'] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get('vim.wo.foldmethod'), 'manual')
  MiniTest.expect.equality(child.lua_get('vim.wo.foldenable'), false)

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
  MiniTest.expect.equality(count > 0, true, 'expected at least one folding range')
end

T['status buffer is read-only to native edits'] = function()
  setup_child()
  local before = child.lua_get('vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)')

  local function opt(name)
    return child.lua_get(string.format("vim.api.nvim_get_option_value(%q, { buf = STATUS.buf })", name))
  end
  MiniTest.expect.equality(opt('buftype'), 'nofile')
  MiniTest.expect.equality(opt('bufhidden'), 'hide')
  MiniTest.expect.equality(opt('swapfile'), false)
  MiniTest.expect.equality(opt('modifiable'), false)
  -- `readonly` must stay unset: a readonly buffer warns W10 on every
  -- programmatic render (bug 4).
  MiniTest.expect.equality(opt('readonly'), false)
  MiniTest.expect.equality(opt('undolevels'), -1)
  MiniTest.expect.equality(opt('modified'), false)

  MiniTest.expect.equality(child.lua_get('HELPERS.wait_idle()'), true)
  -- Non-vacuity: enable modifiable only for the guard's actual native x.
  -- It snapshots the successful edit and restores in a finally-style cleanup
  -- before returning, without yielding to a render or retrying input.
  local guard_ok, guard_err = pcall(type_edit, 'x', { native = true, control = true })
  MiniTest.expect.equality(guard_ok, false,
    { fail_reason = 'the guard must reject a successful edit: ' .. tostring(guard_err) })
  MiniTest.expect.equality(tostring(guard_err):find('unchanged after x', 1, true) ~= nil, true,
    { fail_reason = 'guard failure must be the buffer-unchanged check: ' .. tostring(guard_err) })
  MiniTest.expect.equality(child.lua_get('HELPERS.lines()'), before,
    { fail_reason = 'mutation must be fully restored' })
  MiniTest.expect.equality(opt('modifiable'), false)
  MiniTest.expect.equality(opt('modified'), false)

  -- `u` is bound to unstage and is a no-op with the cursor on the header, so
  -- it is classified as a bound no-op (no E21 expected).
  type_edit('u', { bound_noop = true })
  -- `J` and `~` are unbound in every profile, so they must fail with E21.
  type_edit('J')
  type_edit('~')
  -- `c` opens the commit view, so `ciw<Esc>` is not an unbound edit; type the
  -- native `ciw` with mappings bypassed to assert E21 honestly.
  type_edit('ciw<Esc>', { native = true })

  local ok, err = pcall(child.cmd, 's/a/b/')
  MiniTest.expect.equality(ok, false, { fail_reason = 'substitute should be refused' })
  MiniTest.expect.equality(
    tostring(err):find('E21', 1, true) ~= nil,
    true,
    { fail_reason = 'substitute should fail with E21: ' .. tostring(err) }
  )
  child.v.errmsg = ''

  local after = child.lua_get('vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)')
  MiniTest.expect.equality(after, before)
  MiniTest.expect.equality(opt('modified'), false)
end

T['refresh and mutations still work after blocked edits'] = function()
  setup_child()
  -- `d` now opens the diff menu (Magit's key), so use native edits that are
  -- unbound in every profile.
  type_edit('J')
  type_edit('~')

  local path = repo .. '/f.txt'
  local lines = vim.fn.readfile(path)
  table.insert(lines, 'line11')
  vim.fn.writefile(lines, path)

  child.lua('M.refresh()')
  assert_line('    +line11')
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('modified', { buf = STATUS.buf })"),
    false
  )

  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(invoke_at(target, 'status.stage'), true)
  assert_line('Staged changes (1)')
end

T['write is a no-op and keeps the buffer unmodified'] = function()
  setup_child()
  child.lua([[
    HINTS = {}
    vim.notify = function(msg, level)
      table.insert(HINTS, { msg = msg, level = level })
    end
  ]])
  local before = child.lua_get('vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)')

  local ok, err = pcall(child.type_keys, 100, ':write<CR>')
  MiniTest.expect.equality(ok, true, 'write must not raise: ' .. tostring(err))
  -- Wait until the queued `:write` actually ran (its hint is the observable
  -- result) before reading `v:errmsg` below.
  MiniTest.expect.equality(
    child.lua_get("HELPERS.wait_for(function() return #HINTS >= 1 end, 2000)"),
    true,
    'write must reach the read-only hint'
  )
  MiniTest.expect.equality(child.v.errmsg, '', 'write must not produce a Vim error')
  child.v.errmsg = ''

  MiniTest.expect.equality(child.lua_get('#HINTS'), 1)
  local msg = child.lua_get('HINTS[1].msg')
  MiniTest.expect.equality(type(msg), 'string')
  MiniTest.expect.equality(msg:find('Read-only view', 1, true) ~= nil, true, msg)
  MiniTest.expect.equality(child.lua_get('HINTS[1].level'), vim.log.levels.INFO)

  child.type_keys(100, ':write<CR>')
  -- The second write must not add a hint; poll for the violation until the
  -- queued `:write` has had time to run, so absence is a real observation.
  MiniTest.expect.equality(
    child.lua_get("HELPERS.wait_for(function() return #HINTS > 1 end, 500)"),
    false,
    'the hint is shown once per session'
  )

  local after = child.lua_get('vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)')
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

T['navigation jumps follow document symbols'] = function()
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
  MiniTest.expect.equality(#symbols > 0, true, 'documentSymbol returns items')

  local staged_line, unstaged_section_line, unstaged_line, hunk_line
  for i, line in ipairs(child.lua_get('STATUS.lines')) do
    if line == 'Staged changes (0)' then
      staged_line = i
    elseif line == 'Unstaged changes (1)' then
      unstaged_section_line = i
    elseif line == '  M f.txt' then
      unstaged_line = i
    elseif line == '    @@ -1,10 +1,10 @@' then
      hunk_line = i
    end
  end

  child.lua('vim.api.nvim_win_set_cursor(0, { 1, 0 }); J = M.jump("section", 1)')
  MiniTest.expect.equality(child.lua_get('J'), staged_line)
  child.lua('vim.api.nvim_win_set_cursor(0, { 1, 0 }); J = M.jump("file", 1)')
  MiniTest.expect.equality(child.lua_get('J'), unstaged_line)
  child.lua('vim.api.nvim_win_set_cursor(0, { 1, 0 }); J = M.jump("hunk", 1)')
  MiniTest.expect.equality(child.lua_get('J'), hunk_line)

  child.lua('vim.api.nvim_win_set_cursor(0, { vim.fn.line("$"), 0 }); J = M.jump("section", -1)')
  MiniTest.expect.equality(child.lua_get('J'), unstaged_section_line)
end

T['gO outline lists document symbols'] = function()
  setup_child()
  local items = child.lua_get([[
    (function()
      local captured
      vim.lsp.buf.document_symbol({ on_list = function(t) captured = t.items end })
      vim.wait(5000, function() return captured ~= nil end, 20)
      return captured or {}
    end)()
  ]])
  MiniTest.expect.equality(#items > 0, true, 'the outline has symbols')
  local has_section = false
  for _, item in ipairs(items) do
    if item.text:find('Unstaged changes', 1, true) then
      has_section = true
    end
  end
  MiniTest.expect.equality(has_section, true)
end

T['<CR> on a change line opens the real file at the new-side line'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'line3 CHANGED', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<CR>')

  local opened = child.lua_get("HELPERS.wait_for(function() return vim.api.nvim_buf_get_name(0):match('f%.txt$') ~= nil end)")
  MiniTest.expect.equality(opened, true, 'the worktree file opened')
  MiniTest.expect.equality(child.lua_get('vim.fn.line(".")'), 3)
end

T['rapid successive invokes keep the buffer consistent with the server'] = function()
  setup_child()
  child.lua([[
    for _ = 1, 3 do
      M.invoke('status.stage-all')
      M.invoke('status.unstage-all')
    end
    M.invoke('status.stage-all')
  ]])

  local buffer = child.lua_get('HELPERS.lines()')
  local resync = child.lua_get('HELPERS.resync()')
  MiniTest.expect.equality(buffer, resync.text)
  MiniTest.expect.equality(child.lua_get('STATUS.version'), resync.version)
  assert_line('Staged changes (1)')
end

T['a lost update triggers a full resync'] = function()
  setup_child()
  -- Pretend the client missed a render: the next edits no longer match.
  child.lua('STATUS.version = STATUS.version - 1')
  child.lua("M.invoke('status.stage-all')")

  local buffer = child.lua_get('HELPERS.lines()')
  local resync = child.lua_get('HELPERS.resync()')
  MiniTest.expect.equality(buffer, resync.text)
  MiniTest.expect.equality(child.lua_get('STATUS.version'), resync.version)
  assert_line('Staged changes (1)')
end

T['buffer-local maps match the server keymap (vim profile)'] = function()
  setup_child()
  for _, mode in ipairs({ 'n', 'x' }) do
    local diff = child.lua_get(string.format('HELPERS.keymap_diff(%q)', mode))
    MiniTest.expect.equality(diff.missing, {}, 'missing maps in mode ' .. mode)
    MiniTest.expect.equality(diff.extra, {}, 'stale maps in mode ' .. mode)
  end
end

T['buffer-local maps match the server keymap (standard profile)'] = function()
  setup_child({ profile = 'standard' })
  for _, mode in ipairs({ 'n', 'x' }) do
    local diff = child.lua_get(string.format('HELPERS.keymap_diff(%q)', mode))
    MiniTest.expect.equality(diff.missing, {}, 'missing maps in mode ' .. mode)
    MiniTest.expect.equality(diff.extra, {}, 'stale maps in mode ' .. mode)
  end
end

T['a second view/keymap replaces the applied maps'] = function()
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
  MiniTest.expect.equality(normal['u'], nil, 'the old unstage map is gone')
  MiniTest.expect.equality(normal['gr'], nil, 'the old chord is gone')
  MiniTest.expect.equality(normal['s'] ~= nil, true, 'the new map is applied')
end

T['pressing s stages the change under the cursor'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, 's')

  local staged = child.lua_get("HELPERS.wait_for(function() return HELPERS.has_line('Staged changes (1)') end)")
  MiniTest.expect.equality(staged, true)
  MiniTest.expect.equality(
    git_out({ 'diff', '--cached', '-U3', '--', 'f.txt' }):find('+NEW line', 1, true) ~= nil,
    true
  )
end

T['the g r chord refreshes the view'] = function()
  setup_child()
  local path = repo .. '/f.txt'
  local lines = vim.fn.readfile(path)
  table.insert(lines, 'line11')
  vim.fn.writefile(lines, path)

  child.type_keys(50, 'g', 'r')
  local refreshed = child.lua_get("HELPERS.wait_for(function() return HELPERS.has_line('    +line11') end)")
  MiniTest.expect.equality(refreshed, true)
end

T['Tab toggles a file section through the server'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'file line index')
  local before = child.lua_get('vim.api.nvim_buf_line_count(STATUS.buf)')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<Tab>')

  local collapsed = child.lua_get(string.format(
    "HELPERS.wait_for(function() return vim.api.nvim_buf_line_count(STATUS.buf) < %d end)",
    before
  ))
  MiniTest.expect.equality(collapsed, true, 'the file body collapsed')
  assert_no_line('    +NEW line')
end

T['an unbound key passes through natively'] = function()
  setup_child()
  child.api.nvim_win_set_cursor(0, { 1, 0 })
  child.type_keys(50, 'l')
  MiniTest.expect.equality(child.lua_get('vim.fn.col(".")'), 2)
end

T['Escape with a selection passes through and leaves visual mode'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, 'V', '<Esc>')
  MiniTest.expect.equality(child.lua_get('vim.fn.mode()'), 'n')
end

T['mirrors a user mapping whose rhs is a fold command'] = function()
  child.cmd('nnoremap <F2> za')
  setup_child()
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['<F2>'] ~= nil"), true)

  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  local before = child.lua_get('vim.api.nvim_buf_line_count(STATUS.buf)')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<F2>')

  local collapsed = child.lua_get(string.format(
    "HELPERS.wait_for(function() return vim.api.nvim_buf_line_count(STATUS.buf) < %d end)",
    before
  ))
  MiniTest.expect.equality(collapsed, true, 'the mirrored fold key collapsed the file')
end

T['a user Tab fold mapping toggles the section'] = function()
  child.cmd('nnoremap <Tab> za')
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  local before = child.lua_get('vim.api.nvim_buf_line_count(STATUS.buf)')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<Tab>')

  local collapsed = child.lua_get(string.format(
    "HELPERS.wait_for(function() return vim.api.nvim_buf_line_count(STATUS.buf) < %d end)",
    before
  ))
  MiniTest.expect.equality(collapsed, true, 'the user Tab mapping toggled the file')
end

T['fold_keys mirrors a Lua-function fold mapping'] = function()
  setup_child({ fold_keys = { ['<F3>'] = 'common.toggle-section' } })
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['<F3>'] ~= nil"), true)

  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  local before = child.lua_get('vim.api.nvim_buf_line_count(STATUS.buf)')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<F3>')

  local collapsed = child.lua_get(string.format(
    "HELPERS.wait_for(function() return vim.api.nvim_buf_line_count(STATUS.buf) < %d end)",
    before
  ))
  MiniTest.expect.equality(collapsed, true, 'the configured fold key collapsed the file')
end

T['capture mode forwards printable keys and shows the server hint'] = function()
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
    vim.lsp.buf_request_sync = function(_, method, params)
      if method == 'view/key' then
        SENT[#SENT + 1] = params.key
        return {
          [1] = {
            result = {
              effects = {
                { type = 'message', level = 3, text = 'Unbound key: ' .. params.key },
              },
            },
          },
        }
      end
      return nil
    end
    NOTIFIES = {}
    vim.notify = function(msg, level)
      table.insert(NOTIFIES, { msg = msg, level = level })
    end
  ]])

  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['Q'] ~= nil"), true)
  local before = child.lua_get('vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)')
  child.type_keys(50, 'Q')

  MiniTest.expect.equality(child.lua_get('SENT'), { 'Q' })
  MiniTest.expect.equality(child.lua_get('#NOTIFIES'), 1)
  local msg = child.lua_get('NOTIFIES[1].msg')
  MiniTest.expect.equality(msg:find('Unbound key: Q', 1, true) ~= nil, true, msg)
  local after = child.lua_get('vim.api.nvim_buf_get_lines(STATUS.buf, 0, -1, false)')
  MiniTest.expect.equality(after, before)
end

T['visit_key overrides the planned common.visit binding'] = function()
  setup_child({ visit_key = '<F4>' })
  local target = child.lua_get("HELPERS.find_line('+', 'line3 CHANGED', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { target, 0 })

  -- `common.visit` is Planned, so Enter must not leave the view. The queued
  -- native input needs a real processing barrier before asserting the buffer
  -- is unchanged: poll for the violation (leaving the view) so absence is
  -- observed, not assumed.
  child.type_keys(50, '<CR>')
  local left = child.lua_get("HELPERS.wait_for(function() return vim.api.nvim_buf_get_name(0):match('magit://') == nil end, 500)")
  MiniTest.expect.equality(left, false, 'Enter stays in the view until common.visit is implemented')

  child.type_keys(50, '<F4>')
  local opened = child.lua_get("HELPERS.wait_for(function() return vim.api.nvim_buf_get_name(0):match('f%.txt$') ~= nil end)")
  MiniTest.expect.equality(opened, true, 'the configured visit key opened the file')
  MiniTest.expect.equality(child.lua_get('vim.fn.line(".")'), 3)
end

T['ctrl+t makes the buffer editable and ctrl+t restores the server text'] = function()
  setup_child()
  local before = child.lua_get('STATUS.lines')
  local undo = child.lua_get("require('magit-lsp.buffer').undo_levels(STATUS.buf)")
  MiniTest.expect.equality(undo ~= -1, true, 'the user undo setting was captured')

  local function opt(name)
    return child.lua_get(string.format("vim.api.nvim_get_option_value(%q, { buf = STATUS.buf })", name))
  end
  local function mode_is(wanted)
    return vim.wait(5000, function()
      return child.lua_get('STATUS.mode') == wanted
    end, 20)
  end

  child.type_keys(50, '<C-t>')
  MiniTest.expect.equality(mode_is('text'), true, 'ctrl+t entered text mode')
  MiniTest.expect.equality(opt('modifiable'), true)
  MiniTest.expect.equality(opt('undolevels'), undo)
  MiniTest.expect.equality(
    child.lua_get("#vim.api.nvim_buf_get_keymap(STATUS.buf, 'n')"),
    1,
    'only the toggle stays mapped in text mode'
  )
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['s'] == nil"), true)

  child.type_keys(50, 'dd')
  child.type_keys(50, 'ihello<Esc>')
  -- Wait for the queued edits to land in the buffer instead of a fixed delay.
  MiniTest.expect.equality(
    child.lua_get("HELPERS.wait_for(function() return vim.deep_equal(HELPERS.lines(), STATUS.lines) == false end)"),
    true,
    'the buffer is editable'
  )

  child.type_keys(50, '<C-t>')
  MiniTest.expect.equality(mode_is('interactive'), true, 'ctrl+t left text mode')
  vim.wait(5000, function()
    return child.lua_get('STATUS.lines') ~= nil
  end, 20)
  MiniTest.expect.equality(child.lua_get('HELPERS.lines()'), before, 'the server text is back')
  MiniTest.expect.equality(opt('modifiable'), false)
  MiniTest.expect.equality(opt('readonly'), false)
  MiniTest.expect.equality(opt('undolevels'), -1)
  MiniTest.expect.equality(opt('modified'), false)

  child.lua('pcall(vim.cmd, "silent! undo")')
  MiniTest.expect.equality(
    child.lua_get('HELPERS.lines()'),
    before,
    'undo cannot restore the stale text-mode edits'
  )
end

T['y y copies the value at point to the clipboard'] = function()
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
      return child.lua_get([[vim.fn.getreg('+')]]) ~= ''
    end, 20)
  end

  clear()
  local added = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(added), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { added, 0 })
  child.type_keys(50, 'yy')
  MiniTest.expect.equality(copied(), true, 'the line was copied')
  MiniTest.expect.equality(child.lua_get([[vim.fn.getreg('+')]]), 'NEW line')
  MiniTest.expect.equality(child.lua_get([[vim.fn.getreg('"')]]), 'NEW line')

  clear()
  local hunk = child.lua_get("HELPERS.find_first('hunk', 'unstaged')")
  child.api.nvim_win_set_cursor(0, { hunk, 0 })
  child.type_keys(50, 'yy')
  MiniTest.expect.equality(copied(), true, 'the hunk was copied')
  local hunk_text = child.lua_get([[vim.fn.getreg('+')]])
  MiniTest.expect.equality(hunk_text:find('@@ -1,10 +1,10 @@', 1, true), 1, hunk_text)
  MiniTest.expect.equality(hunk_text:find('\n+NEW line', 1, true) ~= nil, true, hunk_text)

  clear()
  local file = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  child.api.nvim_win_set_cursor(0, { file, 0 })
  child.type_keys(50, 'yy')
  MiniTest.expect.equality(copied(), true, 'the path was copied')
  MiniTest.expect.equality(child.lua_get([[vim.fn.getreg('+')]]), 'f.txt')
end

--- Wait for the most recently opened menu and return its window state.
local function wait_menu()
  local state = false
  local ok = vim.wait(5000, function()
    state = child.lua_get([==[(function()
      local v = require('magit-lsp.view')
      local id
      for index = #v.order, 1, -1 do
        local record = v.views[v.order[index]]
        if record and record.placement == 'bottom' then
          id = record.key
          break
        end
      end
      if not id then return false end
      local record = v.views[id]
      if not record or not record.win or not vim.api.nvim_win_is_valid(record.win)
        or record.version == nil or not record.keymap_ready then
        return false
      end
      local lines = vim.api.nvim_buf_get_lines(record.buf, 0, -1, false)
      return {
        id = id,
        win = record.win,
        buf = record.buf,
        config = vim.api.nvim_win_get_config(record.win),
        width = vim.api.nvim_win_get_width(record.win),
        height = vim.api.nvim_win_get_height(record.win),
        lines = lines,
        current = vim.api.nvim_get_current_win(),
        cap = math.max(1, vim.o.lines - vim.o.cmdheight - 1),
        split_cap = math.max(1, vim.o.lines - vim.o.cmdheight - 3),
        screen_lines = vim.o.lines,
        cmdheight = vim.o.cmdheight,
      }
    end)()]==])
    return type(state) == 'table'
  end, 20)
  return ok and state or nil
end

--- The current lines of menu `id`, or an empty list when it closed.
local function menu_lines(id)
  return child.lua_get(string.format([[(function()
    local record = require('magit-lsp.view').views[%q]
    if not record then return {} end
    return vim.api.nvim_buf_get_lines(record.buf, 0, -1, false)
  end)()]], id))
end

local function menu_closed(id)
  return child.lua_get(string.format(
    "(function() return require('magit-lsp.view').views[%q] == nil end)()",
    id
  ))
end

local function has_prefix(lines, prefix)
  for _, line in ipairs(lines) do
    if vim.startswith(line, prefix) then
      return true
    end
  end
  return false
end

T['pressing c opens the commit menu in a bottom float'] = function()
  setup_child()
  child.type_keys(50, 'c')
  local state = wait_menu()
  MiniTest.expect.equality(type(state), 'table', 'the commit menu opened')

  local height = math.min(#state.lines, state.cap)
  MiniTest.expect.equality(state.config.relative, 'editor')
  MiniTest.expect.equality(state.config.row, state.screen_lines - height - state.cmdheight - 1)
  MiniTest.expect.equality(state.config.col, 0)
  MiniTest.expect.equality(state.config.width, child.lua_get('vim.o.columns'))
  MiniTest.expect.equality(state.config.height, height)
  MiniTest.expect.equality(state.current, state.win, 'the menu is entered')

  MiniTest.expect.equality(state.lines[1], ' Commit')
  MiniTest.expect.equality(vim.tbl_contains(state.lines, ' Arguments'), true, 'the arguments group')
  MiniTest.expect.equality(vim.tbl_contains(state.lines, ' git commit'), true, 'the preview line')
  MiniTest.expect.equality(has_prefix(state.lines, '  c  Commit'), true, 'the Commit action')

  child.type_keys(50, '-a')
  local toggled = vim.wait(5000, function()
    for _, line in ipairs(menu_lines(state.id)) do
      if line:find('%-%-all', 1) and line:find('%[x%]', 1) then
        return true
      end
    end
    return false
  end, 20)
  MiniTest.expect.equality(toggled, true, 'the --all switch shows [x] on screen')

  child.type_keys(50, '<Esc>')
  MiniTest.expect.equality(
    vim.wait(5000, function()
      return menu_closed(state.id)
    end, 20),
    true,
    'Escape closed the menu'
  )
  MiniTest.expect.equality(
    child.lua_get('vim.api.nvim_get_current_win()'),
    child.lua_get('vim.fn.bufwinid(STATUS.buf)'),
    'focus returns to the status window'
  )
end

T['the longest menu fills the editor and scrolls when it is taller'] = function()
  setup_child()
  child.lua('vim.o.columns = 80; vim.o.lines = 24')
  -- The log menu is the longest at 80 columns; `g l` opens it in the vim profile.
  child.type_keys(50, 'g', 'l')
  local state = wait_menu()
  MiniTest.expect.equality(type(state), 'table', 'the log menu opened')
  MiniTest.expect.equality(state.width, 80)
  MiniTest.expect.equality(state.screen_lines, 24)

  local available = state.screen_lines - state.cmdheight - 1
  MiniTest.expect.equality(state.height, math.min(#state.lines, available))
  MiniTest.expect.equality(
    #state.lines > state.height,
    true,
    'the log menu is taller than the float'
  )

  -- C-e is not a captured menu key, so the float scrolls natively.
  child.type_keys(50, '<C-e>')
  local top = child.lua_get(string.format(
    '(function() return vim.api.nvim_win_call(%d, function() return vim.fn.line("w0") end) end)()',
    state.win
  ))
  MiniTest.expect.equality(top > 1, true, 'C-e scrolls the menu float')

  child.api.nvim_win_set_cursor(state.win, { #state.lines, 0 })
  local reached = child.lua_get(string.format(
    '(function() return vim.api.nvim_win_call(%d, function() return vim.fn.line(".") == vim.fn.line("$") end) end)()',
    state.win
  ))
  MiniTest.expect.equality(reached, true, 'the cursor reaches the last line')

  child.type_keys(50, '<Esc>')
  MiniTest.expect.equality(
    vim.wait(5000, function()
      return menu_closed(state.id)
    end, 20),
    true,
    'Escape closed the log menu'
  )
end

T['menu.style split opens a bottom split'] = function()
  setup_child({ menu = { style = 'split' } })
  child.type_keys(50, 'c')
  local state = wait_menu()
  MiniTest.expect.equality(type(state), 'table', 'the commit menu opened')

  local height = math.min(#state.lines, state.split_cap)
  MiniTest.expect.equality(state.config.relative, '')
  MiniTest.expect.equality(state.height, height)
  local bottom = child.lua_get(string.format(
    'vim.fn.win_screenpos(%d)[1] + vim.fn.winheight(%d) - 1',
    state.win,
    state.win
  ))
  MiniTest.expect.equality(bottom, state.screen_lines - state.cmdheight - 1)

  child.type_keys(50, '<Esc>')
  MiniTest.expect.equality(
    vim.wait(5000, function()
      return menu_closed(state.id)
    end, 20),
    true,
    'Escape closed the split menu'
  )
end

--- Replace every string equal to `secret` with a marker, so a failed equality
--- below never prints the synthetic secret into the failure diagnostics.
local function redact(value, secret)
  if type(value) == 'table' then
    local out = {}
    for key, item in pairs(value) do
      out[key] = redact(item, secret)
    end
    return out
  end
  if type(value) == 'string' and value == secret then
    return '<redacted>'
  end
  return value
end

T['every prompt kind answers view/promptRespond with its §3.6 shape'] = function()
  setup_child()
  child.lua([[
    -- Drive one synthetic `view/prompt` through the real notification handler
    -- with a fake capturing client and per-row `vim.ui`/`vim.fn` stubs.
    RUN_PROMPT = function(scenario)
      local debug_api = require('magit-lsp.debug')
      local sent = {}
      local calls = { input = {}, select = {}, inputsecret = {} }
      local real_client = vim.lsp.get_client_by_id
      vim.lsp.get_client_by_id = function(id)
        if id == 777 then
          return {
            request = function(_, method, params, handler)
              sent[#sent + 1] = { method = method, params = params }
              if handler then handler(nil, {}) end
            end,
          }
        end
        return real_client(id)
      end
      local select_answers = {}
      for index, answer in ipairs(scenario.selectAnswers or {}) do
        select_answers[index] = answer
      end
      vim.ui.input = function(opts, on_confirm)
        calls.input[#calls.input + 1] = { prompt = opts.prompt, default = opts.default }
        on_confirm(scenario.inputAnswer)
      end
      vim.fn.inputsecret = function(title)
        calls.inputsecret[#calls.inputsecret + 1] = title
        return scenario.secretAnswer
      end
      vim.ui.select = function(items, opts, on_confirm)
        local entry = { prompt = opts.prompt, items = vim.deepcopy(items) }
        if opts.format_item then
          entry.formatted = vim.tbl_map(opts.format_item, items)
        end
        calls.select[#calls.select + 1] = entry
        local answer = table.remove(select_answers, 1)
        if answer == nil then
          on_confirm(nil)
          return
        end
        if type(answer) == 'number' then
          on_confirm(items[answer])
          return
        end
        for _, item in ipairs(items) do
          if (type(item) == 'table' and item.id or item) == answer then
            on_confirm(item)
            return
          end
        end
        on_confirm(nil)
      end
      vim.lsp.handlers['view/prompt'](nil, scenario.params, { client_id = 777 })
      vim.lsp.get_client_by_id = real_client
      return { sent = sent, calls = calls, prompts_pending = debug_api.state.prompts }
    end
  ]])

  local multi_items = {
    { id = 'a', label = 'a' },
    { id = 'b', label = 'b', description = 'desc b' },
    { id = 'c', label = 'c' },
  }
  local scenarios = {
    {
      name = 'text success',
      params = { promptId = 'p-text-ok', kind = 'text', title = 'Author name?', default = 'seed' },
      inputAnswer = 'Alice',
      want = { promptId = 'p-text-ok', cancelled = false, value = 'Alice' },
      want_input = { prompt = 'Author name?', default = 'seed' },
    },
    {
      name = 'text empty answer',
      params = { promptId = 'p-text-empty', kind = 'text', title = 'Name?' },
      inputAnswer = '',
      want = { promptId = 'p-text-empty', cancelled = false, value = '' },
      want_input = { prompt = 'Name?' },
    },
    {
      name = 'text cancel',
      params = { promptId = 'p-text-cancel', kind = 'text', title = 'Name?' },
      want = { promptId = 'p-text-cancel', cancelled = true, value = nil },
      want_input = { prompt = 'Name?' },
    },
    {
      name = 'confirm yes',
      params = { promptId = 'p-confirm-yes', kind = 'confirm', title = 'Delete branch feature/x?' },
      selectAnswers = { 1 },
      want = { promptId = 'p-confirm-yes', cancelled = false, value = true },
      want_select_items = { 'Yes', 'No' },
    },
    {
      name = 'confirm no',
      params = { promptId = 'p-confirm-no', kind = 'confirm', title = 'Delete branch feature/x?' },
      selectAnswers = { 2 },
      want = { promptId = 'p-confirm-no', cancelled = false, value = false },
    },
    {
      name = 'confirm cancel',
      params = { promptId = 'p-confirm-cancel', kind = 'confirm', title = 'Delete branch feature/x?' },
      want = { promptId = 'p-confirm-cancel', cancelled = true, value = nil },
    },
    {
      name = 'confirm destructive focuses No first',
      params = {
        promptId = 'p-confirm-destructive',
        kind = 'confirm',
        title = 'Drop the stash?',
        yesLabel = 'Keep it',
        noLabel = 'Drop it',
        destructive = true,
      },
      selectAnswers = { 1 },
      want = { promptId = 'p-confirm-destructive', cancelled = false, value = false },
      want_select_items = { 'Drop it', 'Keep it' },
    },
    {
      name = 'choice answers a fixed id',
      params = {
        promptId = 'p-choice',
        kind = 'choice',
        title = 'Which base?',
        items = {
          { id = 'head', label = 'HEAD' },
          { id = 'branch-x', label = 'branch x', description = '2 commits ahead' },
        },
      },
      selectAnswers = { 'branch-x' },
      want = { promptId = 'p-choice', cancelled = false, value = { id = 'branch-x' } },
      want_select_items = {
        { id = 'head', label = 'HEAD' },
        { id = 'branch-x', label = 'branch x', description = '2 commits ahead' },
      },
      want_select_formatted = { 'HEAD', 'branch x — 2 commits ahead' },
    },
    {
      name = 'choice cancel',
      params = {
        promptId = 'p-choice-cancel',
        kind = 'choice',
        title = 'Which base?',
        items = {
          { id = 'head', label = 'HEAD' },
          { id = 'branch-x', label = 'branch x', description = '2 commits ahead' },
        },
      },
      want = { promptId = 'p-choice-cancel', cancelled = true, value = nil },
    },
    {
      name = 'choice with allowCustom types a custom value',
      params = {
        promptId = 'p-choice-custom',
        kind = 'choice',
        title = 'Which base?',
        allowCustom = true,
        items = { { id = 'head', label = 'HEAD' } },
      },
      selectAnswers = { '__custom__' },
      inputAnswer = 'refs/heads/wip',
      want = { promptId = 'p-choice-custom', cancelled = false, value = { custom = 'refs/heads/wip' } },
      want_input = { prompt = 'Which base?' },
      want_select_items = {
        { id = 'head', label = 'HEAD' },
        { id = '__custom__', label = 'Enter custom value…' },
      },
    },
    {
      name = 'choice with allowCustom can cancel the custom input',
      params = {
        promptId = 'p-choice-custom-cancel',
        kind = 'choice',
        title = 'Which base?',
        allowCustom = true,
        items = { { id = 'head', label = 'HEAD' } },
      },
      selectAnswers = { '__custom__' },
      want = { promptId = 'p-choice-custom-cancel', cancelled = true, value = nil },
      want_input = { prompt = 'Which base?' },
    },
    {
      name = 'multiChoice toggles preselected items and answers Done',
      params = {
        promptId = 'p-multi',
        kind = 'multiChoice',
        title = 'Pick branches',
        items = multi_items,
        preselected = { 'a' },
      },
      selectAnswers = { 'b', 'a', '__done__' },
      want = { promptId = 'p-multi', cancelled = false, value = { ids = { 'b' } } },
      want_select_steps = {
        { '[x] a', '[ ] b — desc b', '[ ] c', 'Done' },
        { '[x] a', '[x] b — desc b', '[ ] c', 'Done' },
        { '[ ] a', '[x] b — desc b', '[ ] c', 'Done' },
      },
    },
    {
      name = 'multiChoice answers an empty selection',
      params = {
        promptId = 'p-multi-empty',
        kind = 'multiChoice',
        title = 'Pick branches',
        items = { { id = 'a', label = 'a' } },
      },
      selectAnswers = { '__done__' },
      want = { promptId = 'p-multi-empty', cancelled = false, value = { ids = {} } },
    },
    {
      name = 'multiChoice cancel',
      params = {
        promptId = 'p-multi-cancel',
        kind = 'multiChoice',
        title = 'Pick branches',
        items = { { id = 'a', label = 'a' } },
      },
      want = { promptId = 'p-multi-cancel', cancelled = true, value = nil },
    },
    {
      name = 'secret success',
      params = { promptId = 'p-secret-ok', kind = 'text', title = 'SSH passphrase', secret = true },
      secretAnswer = 'correct horse battery',
      secret = 'correct horse battery',
      want = { promptId = 'p-secret-ok', cancelled = false, value = '<redacted>' },
      want_secret_titles = { 'SSH passphrase' },
      want_no_input = true,
    },
    {
      name = 'secret empty answer cancels',
      params = { promptId = 'p-secret-empty', kind = 'text', title = 'SSH passphrase', secret = true },
      secretAnswer = '',
      want = { promptId = 'p-secret-empty', cancelled = true, value = nil },
      want_secret_titles = { 'SSH passphrase' },
      want_no_input = true,
    },
    {
      name = 'secret cancel',
      params = { promptId = 'p-secret-cancel', kind = 'text', title = 'SSH passphrase', secret = true },
      want = { promptId = 'p-secret-cancel', cancelled = true, value = nil },
      want_secret_titles = { 'SSH passphrase' },
      want_no_input = true,
    },
  }

  for _, scenario in ipairs(scenarios) do
    local label = scenario.name
    child.lua(string.format('RESULT = RUN_PROMPT(%s)', vim.inspect(scenario)))
    local result = child.lua_get('RESULT')
    MiniTest.expect.equality(type(result), 'table', label .. ': the prompt was handled')
    if type(result) == 'table' then
      MiniTest.expect.equality(#result.sent, 1, label .. ': exactly one respond request')
      if result.sent[1] then
        MiniTest.expect.equality(
          result.sent[1].method,
          'view/promptRespond',
          label .. ': the answer method'
        )
        local got, want = result.sent[1].params, scenario.want
        if scenario.secret then
          got, want = redact(got, scenario.secret), redact(want, scenario.secret)
        end
        MiniTest.expect.equality(got, want, label .. ': the respond payload')
      end
      MiniTest.expect.equality(
        result.prompts_pending,
        0,
        label .. ': the prompt counter is back to zero'
      )

      if scenario.want_input then
        MiniTest.expect.equality(
          result.calls.input[1],
          scenario.want_input,
          label .. ': the vim.ui.input options'
        )
        MiniTest.expect.equality(#result.calls.input, 1, label .. ': one vim.ui.input call')
      end
      if scenario.want_no_input then
        MiniTest.expect.equality(#result.calls.input, 0, label .. ': vim.ui.input was never used')
      end
      if scenario.want_secret_titles then
        MiniTest.expect.equality(
          result.calls.inputsecret,
          scenario.want_secret_titles,
          label .. ': inputsecret got the title'
        )
      else
        MiniTest.expect.equality(
          #result.calls.inputsecret,
          0,
          label .. ': inputsecret was never used'
        )
      end

      if scenario.want_select_items then
        MiniTest.expect.equality(
          result.calls.select[1].items,
          scenario.want_select_items,
          label .. ': the select items'
        )
        MiniTest.expect.equality(
          result.calls.select[1].prompt,
          scenario.params.title,
          label .. ': the select prompt'
        )
      end
      if scenario.want_select_formatted then
        MiniTest.expect.equality(
          result.calls.select[1].formatted,
          scenario.want_select_formatted,
          label .. ': the rendered item labels'
        )
      end
      if scenario.want_select_steps then
        MiniTest.expect.equality(
          #result.calls.select,
          #scenario.want_select_steps,
          label .. ': one select call per toggle step'
        )
        for step, want in ipairs(scenario.want_select_steps) do
          MiniTest.expect.equality(
            result.calls.select[step].formatted,
            want,
            label .. ': the step ' .. step .. ' labels'
          )
          MiniTest.expect.equality(
            result.calls.select[step].prompt,
            scenario.params.title,
            label .. ': the step ' .. step .. ' prompt'
          )
        end
      end
    end
  end

  MiniTest.expect.equality(
    child.lua('return vim.wait(2000, function() return require("magit-lsp.debug").idle() end, 10)'),
    true,
    'the client is back to idle'
  )
end

T['a menu option prompt asks through vim.ui.input and lands the value'] = function()
  setup_child()
  child.lua([[
    PROMPTS = {}
    vim.ui.input = function(opts, on_confirm)
      table.insert(PROMPTS, opts.prompt)
      on_confirm('Alice')
    end
  ]])

  child.type_keys(50, 'c')
  local state = wait_menu()
  MiniTest.expect.equality(type(state), 'table', 'the commit menu opened')

  child.type_keys(50, '=A')
  local applied = vim.wait(5000, function()
    for _, line in ipairs(menu_lines(state.id)) do
      if line:find('--author=Alice', 1, true) then
        return true
      end
    end
    return false
  end, 20)
  MiniTest.expect.equality(applied, true, 'the option value and preview land')
  MiniTest.expect.equality(child.lua_get('#PROMPTS'), 1, 'one prompt was shown')
  MiniTest.expect.equality(
    child.lua_get('PROMPTS[1]'),
    'Override the author (--author=)'
  )
end

--- Compatibility alias for `unpack` across Lua versions.
local unpack = table.unpack or unpack

--- Fetch the structured menu model of the view `state` from the server via
--- `view/resync`. The wire request uses the actual `record.id`; the composite
--- `record.key` is only used to look the view record up on the client.
---@param state table the `wait_menu()` state of an open menu
---@return table|nil menu the server's menu model, or nil
local function menu_model(state)
  if not state then return nil end
  return child.lua_get(string.format([[
    (function()
      local record = require('magit-lsp.view').views[%q]
      if not record then return nil end
      local result, err = require('magit-lsp.view').sync(
        record.buf, record.client_id, 'view/resync', { viewId = record.id })
      if err or not result then return nil end
      return result.menu
    end)()
  ]], state.id))
end

--- The first planned action of the menu model, with its key, label and roadmap
--- id derived from the server metadata.
---@param state table the `wait_menu()` state of an open menu
---@return table|nil planned { key, label, roadmap }
local function planned_in(state)
  local menu = menu_model(state)
  if not menu then return nil end
  for _, group in ipairs(menu.groups or {}) do
    for _, item in ipairs(group.items or {}) do
      if item.status and item.status.status == 'planned' then
        return { key = item.key, label = item.label, roadmap = item.status.roadmapId }
      end
    end
  end
  return nil
end

--- Find a real planned menu action without pinning any menu, key or label.
---
--- Tries the commit menu first; when it has no planned action, opens the
--- dispatch menu and tries every opener it lists, so the cases keep working
--- once every commit action is implemented.
---@return table|nil planned { key, label, roadmap }
---@return table|nil state the menu state the planned action was found in
local function find_planned_action()
  child.type_keys(50, 'c')
  local state = wait_menu()
  local planned = planned_in(state)
  if planned then return planned, state end

  child.type_keys(50, 'q')
  child.type_keys(50, 'g?')
  local dispatch = wait_menu()
  planned = planned_in(dispatch)
  if planned then return planned, dispatch end
  if not dispatch then return nil end

  local dispatch_model = menu_model(dispatch)
  if not dispatch_model then return nil end
  for _, group in ipairs(dispatch_model.groups or {}) do
    for _, item in ipairs(group.items or {}) do
      if item.kind == 'action' and item.key then
        local keys = {}
        for key in item.key:gmatch('%S+') do
          keys[#keys + 1] = key
        end
        child.type_keys(50, unpack(keys))
        local opened = wait_menu()
        local found = planned_in(opened)
        if found then return found, opened end
        if opened then
          child.type_keys(50, 'q')
          child.type_keys(50, 'g?')
          wait_menu()
        end
      end
    end
  end
  child.type_keys(50, 'q')
  return nil
end

--- Locate the `magit.menu.planned` token for `label` through the server's
--- semantic-token ranges, never by searching the rendered labels.
---@param view_id string the composite key of the open menu view
---@param label string the label of the planned action to locate
---@return table|nil position { row, col } of the planned token
local function planned_token_position(view_id, label)
  return child.lua_get(string.format([[
    (function()
      local view = require('magit-lsp.view').views[%q]
      if not view then return nil end
      local get_clients = vim.lsp.get_clients or vim.lsp.get_active_clients
      local client = get_clients({ bufnr = view.buf })[1]
      if not client then return nil end
      local capability = client.server_capabilities.semanticTokensProvider
      if not capability then return nil end
      local responses = vim.lsp.buf_request_sync(view.buf,
        'textDocument/semanticTokens/full',
        { textDocument = { uri = view.uri } }, 5000)
      local response = responses and responses[client.id]
      if not response or not response.result then return nil end
      local data = response.result.data
      local line, char = 0, 0
      for i = 1, #data, 5 do
        local delta_line, delta_start, length, token_type =
          data[i], data[i + 1], data[i + 2], data[i + 3]
        line = line + delta_line
        char = delta_line > 0 and delta_start or char + delta_start
        if capability.legend.tokenTypes[token_type + 1] == 'magit.menu.planned'
          and vim.api.nvim_buf_get_text(
            view.buf, line, char, line, char + length, {})[1] == %q then
          return { row = line, col = char }
        end
      end
      return nil
    end)()
  ]], view_id, label))
end

T['planned menu actions are greyed with the menu.planned token'] = function()
  setup_child()
  local planned, state = find_planned_action()
  MiniTest.expect.equality(type(state), 'table', 'a menu with a planned action opened')

  local position = planned_token_position(state.id, planned.label)
  MiniTest.expect.equality(type(position), 'table', 'the planned action token position')

  local token = child.lua_get(string.format([[
    (function()
      local view = require('magit-lsp.view').views[%q]
      local ok = vim.wait(5000, function()
        return vim.lsp.semantic_tokens.get_at_pos(view.buf, %d, %d) ~= nil
      end, 20)
      if not ok then return nil end
      local tokens = vim.lsp.semantic_tokens.get_at_pos(view.buf, %d, %d)
      return tokens and tokens[1] and tokens[1].type or nil
    end)()
  ]], state.id, position.row, position.col, position.row, position.col))
  MiniTest.expect.equality(token, 'magit.menu.planned')
end

T['a planned menu action notifies once and keeps the menu open'] = function()
  setup_child()
  local planned, state = find_planned_action()
  MiniTest.expect.equality(type(state), 'table', 'a menu with a planned action opened')

  child.lua([[
    NOTIFIES = {}
    vim.notify = function(msg, level)
      table.insert(NOTIFIES, { msg = msg, level = level })
    end
  ]])
  local keys = {}
  for key in planned.key:gmatch('%S+') do
    keys[#keys + 1] = key
  end
  child.type_keys(50, unpack(keys))
  vim.wait(5000, function()
    return child.lua_get('#NOTIFIES') > 0
  end, 20)
  MiniTest.expect.equality(child.lua_get('#NOTIFIES'), 1)
  MiniTest.expect.equality(
    child.lua_get('NOTIFIES[1].msg'),
    'Not implemented yet: ' .. planned.label .. ' (roadmap ' .. planned.roadmap .. ')'
  )
  MiniTest.expect.equality(child.lua_get('NOTIFIES[1].level'), vim.log.levels.INFO)
  MiniTest.expect.equality(menu_closed(state.id), false, 'the menu stays open')
end

T['the dispatch menu lists the active profile keys'] = function()
  setup_child()
  child.type_keys(50, 'g?')
  local state = wait_menu()
  MiniTest.expect.equality(type(state), 'table', 'the dispatch menu opened')
  local lines = menu_lines(state.id)
  local function has(text)
    for _, line in ipairs(lines) do
      if line:find(text, 1, true) then
        return true
      end
    end
    return false
  end
  MiniTest.expect.equality(has('  Z  Stash'), true, 'vim binds stash to Z')
  MiniTest.expect.equality(has('  g l  Log'), true, 'vim binds log to g l')
  MiniTest.expect.equality(has('  c  Commit'), true, 'vim binds commit to c')
  MiniTest.expect.equality(has('  g ?  Dispatch'), false, 'dispatch does not list itself')
end

T['? from a menu replaces it and leaves one float'] = function()
  setup_child()
  child.type_keys(50, 'c')
  local commit = wait_menu()
  MiniTest.expect.equality(type(commit), 'table', 'the commit menu opened')

  child.type_keys(50, '?')
  local state
  local replaced = vim.wait(5000, function()
    state = child.lua_get(string.format([==[(function()
      local view = require('magit-lsp.view')
      local debug_api = require('magit-lsp.debug')
      local snapshot = debug_api.snapshot()
      local floats = 0
      for _, window in ipairs(snapshot.windows) do
        if window.float then floats = floats + 1 end
      end
      local last
      for index = #view.order, 1, -1 do
        local record = view.views[view.order[index]]
        if record and record.placement == 'bottom' then
          last = record.key
          break
        end
      end
      return {
        floats = floats,
        last = last,
        old_closed = view.views[%q] == nil,
        current = vim.api.nvim_get_current_win(),
      }
    end)()]==], commit.id))
    return type(state) == 'table'
      and state.floats == 1
      and state.old_closed
      and state.last ~= commit.id
  end, 20)
  MiniTest.expect.equality(replaced, true, 'the commit menu was replaced by one menu')
  MiniTest.expect.equality(
    state.last:find('menu.dispatch', 1, true) ~= nil,
    true,
    'the dispatch menu took its place'
  )

  child.type_keys(50, 'q')
  local closed = vim.wait(5000, function()
    local snapshot = child.lua_get([[(function()
      local view = require('magit-lsp.view')
      local debug_api = require('magit-lsp.debug')
      local state = debug_api.snapshot()
      local floats = 0
      for _, window in ipairs(state.windows) do
        if window.float then floats = floats + 1 end
      end
      local open = 0
      for _, id in ipairs(view.order) do
        local record = view.views[id]
        if record and record.placement == 'bottom' then
          open = open + 1
        end
      end
      return { floats = floats, open = open }
    end)()]])
    return type(snapshot) == 'table' and snapshot.floats == 0 and snapshot.open == 0
  end, 20)
  MiniTest.expect.equality(closed, true, 'q closes the only menu')
  MiniTest.expect.equality(
    child.lua_get('vim.api.nvim_get_current_win()'),
    child.lua_get('vim.fn.bufwinid(STATUS.buf)'),
    'focus returns to the status window'
  )
end

T['debug.snapshot reports the status view state'] = function()
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

  MiniTest.expect.equality(snapshot.mode, 'n')
  MiniTest.expect.equality(type(snapshot.cursor.row), 'number')
  MiniTest.expect.equality(type(snapshot.cursor.col), 'number')
  MiniTest.expect.equality(snapshot.idle, true)
  MiniTest.expect.equality(snapshot.pending, '')

  MiniTest.expect.equality(#snapshot.view_stack, 1)
  MiniTest.expect.equality(snapshot.view_stack[1].id, child.lua_get('STATUS.key'))
  MiniTest.expect.equality(snapshot.view_stack[1].kind, 'status')

  MiniTest.expect.equality(snapshot.render.version, snapshot.render.acked)
  MiniTest.expect.equality(snapshot.render.version > 0, true, 'a version was rendered and acked')

  MiniTest.expect.equality(snapshot.keymap.scope, 'status')
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

  local status_buf = child.lua_get('STATUS.buf')
  local status_window
  for _, window in ipairs(snapshot.windows) do
    if window.buffer == status_buf then
      status_window = window
    end
  end
  MiniTest.expect.equality(type(status_window), 'table', 'the status window is reported')
  MiniTest.expect.equality(status_window.float, false)
  MiniTest.expect.equality(status_window.filetype, 'magit-lsp')
  MiniTest.expect.equality(status_window.width > 0, true)
  MiniTest.expect.equality(status_window.height > 0, true)

  MiniTest.expect.equality(type(snapshot.messages), 'table')
  MiniTest.expect.equality(type(snapshot.notifications), 'table')
end

T['buffers renders that beat the view/open response'] = function()
  child.lua([[
    DELAYED_OPEN = false
    RENDER_BEFORE_RESPONSE = false
    local original = vim.lsp.buf_request
    vim.lsp.buf_request = function(bufnr, method, params, handler)
      if method == 'view/open' and type(handler) == 'function' then
        DELAYED_OPEN = true
        return original(bufnr, method, params, function(...)
          local args = { n = select('#', ...), ... }
          vim.defer_fn(function()
            -- Proves the race: the render was received (and buffered) before
            -- the response handler ran.
            RENDER_BEFORE_RESPONSE = next(VIEW.pending_render) ~= nil
            handler(unpack(args, 1, args.n))
          end, 300)
        end)
      end
      return original(bufnr, method, params, handler)
    end
  ]])
  setup_child()
  MiniTest.expect.equality(child.lua_get('DELAYED_OPEN'), true, 'the open response was delayed')
  MiniTest.expect.equality(
    child.lua_get('RENDER_BEFORE_RESPONSE'),
    true,
    'the render beat the view/open response'
  )

  assert_line('Head: main')
  assert_line('Unstaged changes (1)')
  assert_line('  M f.txt')
  assert_line('    +NEW line')

  local diff = child.lua_get('HELPERS.keymap_diff()')
  MiniTest.expect.equality(diff.missing, {}, 'the early keymap was applied')
  MiniTest.expect.equality(diff.extra, {}, 'the early keymap was applied')

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
  MiniTest.expect.equality(state.version > 0, true, 'the buffered render was applied')
  MiniTest.expect.equality(state.scope, 'status')
end

T['debug.trace writes one JSON line per event'] = function()
  local trace_path = project .. '/build/test-output/trace-lua.jsonl'
  vim.fn.mkdir(project .. '/build/test-output', 'p')
  vim.fn.delete(trace_path)
  child.restart({
    '-u',
    'scripts/minimal_init.lua',
    '--cmd',
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
  MiniTest.expect.equality(#lines > 0, true, 'the trace file has events')
  local seen = {}
  local times = {}
  for _, line in ipairs(lines) do
    local ok, decoded = pcall(vim.json.decode, line)
    MiniTest.expect.equality(ok, true, 'a trace line is JSON: ' .. line)
    MiniTest.expect.equality(type(decoded.t_ms), 'number', 't_ms is a number')
    MiniTest.expect.equality(type(decoded.id), 'number', 'id is a number')
    MiniTest.expect.equality(type(decoded.event), 'string', 'event is a string')
    MiniTest.expect.equality(decoded.detail ~= nil, true, 'detail is present')
    seen[decoded.event] = true
    times[#times + 1] = decoded.t_ms
  end
  for _, name in ipairs({
    'key_pressed',
    'key_sent',
    'render_received',
    'render_applied',
    'window_opened',
    'idle',
  }) do
    MiniTest.expect.equality(seen[name], true, name .. ' was traced')
  end
  for index = 2, #times do
    MiniTest.expect.equality(times[index] >= times[index - 1], true, 'the clock is monotonic')
  end
end

T['two repositories get independent status views'] = function()
  setup_child()
  local other = make_repo(true, project .. '/build/test-repos/fixture-other')
  vim.fn.writefile({ 'other' }, other .. '/other.txt')

  child.lua(string.format("vim.cmd('tabnew'); SECOND = M.setup({ root_dir = %q })", other))
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

  MiniTest.expect.equality(#state, 2, 'both repositories opened a status view')
  local first, second = state[1], state[2]
  MiniTest.expect.equality(first.root, repo)
  MiniTest.expect.equality(second.root, other)
  MiniTest.expect.equality(first.id, second.id, 'independent servers reuse bare view ids')
  MiniTest.expect.equality(first.client_id ~= second.client_id, true)
  MiniTest.expect.equality(first.key ~= second.key, true, 'client/view identities are independent')
  MiniTest.expect.equality(first.buf ~= second.buf, true, 'buffers are independent')
  MiniTest.expect.equality(vim.tbl_contains(first.lines, '  ? other.txt'), false)
  MiniTest.expect.equality(vim.tbl_contains(second.lines, '  ? other.txt'), true)

  -- A render is routed by view id: applying one for the first view must never
  -- rewrite the second buffer.
  child.lua(string.format([[
    local v = require('magit-lsp.view')
    local record = v.views[%q]
    v.apply_render({ viewId = %q, version = (record.version or 0) + 1, text = { 'FIRST ONLY' } }, record.client_id)
  ]], first.key, first.id))
  MiniTest.expect.equality(
    child.lua_get(string.format('vim.api.nvim_buf_get_lines(%d, 0, -1, false)', first.buf)),
    { 'FIRST ONLY' }
  )
  MiniTest.expect.equality(
    child.lua_get(string.format(
      'vim.tbl_contains(vim.api.nvim_buf_get_lines(%d, 0, -1, false), "FIRST ONLY")',
      second.buf
    )),
    false,
    'the second view kept its own text'
  )

  -- Quitting a nested view must focus the existing status record, preserving
  -- its buffer, cursor and folds. The other repository stays live throughout.
  child.lua(string.format([[
    VIEW.focus(%q)
    local render = VIEW.sync(STATUS.buf, STATUS.client_id, 'view/resync', { viewId = STATUS.id })
    VIEW.apply_render(render, STATUS.client_id)
    local parent = VIEW.views[%q]
    vim.api.nvim_win_set_cursor(parent.win, { 8, 2 })
    vim.api.nvim_win_call(parent.win, function() vim.cmd('2,4fold') end)
    PARENT = { key = parent.key, buf = parent.buf, version = parent.version,
      cursor = vim.api.nvim_win_get_cursor(parent.win), fold = vim.fn.foldclosed(3),
      text = parent.lines }
    M.press({ 'g', '$' })
    NESTED = VIEW.current()
    vim.wait(5000, function() return NESTED.version ~= nil and NESTED.keymap_ready end, 10)
    M.press({ 'q' })
    RETURNED = VIEW.current()
    RETURNED_STATE = { key = RETURNED.key, buf = RETURNED.buf, version = RETURNED.version,
      cursor = vim.api.nvim_win_get_cursor(RETURNED.win), fold = vim.fn.foldclosed(3),
      text = RETURNED.lines }
  ]], first.key, first.key))
  MiniTest.expect.equality(child.lua_get('NESTED.placement'), 'main')
  MiniTest.expect.equality(child.lua_get('RETURNED == STATUS'), true)
  MiniTest.expect.equality(child.lua_get('RETURNED_STATE'), child.lua_get('PARENT'))
  MiniTest.expect.equality(child.lua_get('vim.lsp.buf_is_attached(RETURNED.buf, RETURNED.client_id)'), true)
  MiniTest.expect.equality(child.lua_get(string.format('VIEW.views[%q] ~= nil', second.key)), true)
  child.lua([[
    FRESH_BEFORE = RETURNED.version
    M.press({ 'g', 'r' })
    FRESH_REFRESHED = vim.wait(5000, function() return RETURNED.version > FRESH_BEFORE end, 10)
    M.press({ 'c' })
  ]])
  MiniTest.expect.equality(child.lua_get('FRESH_REFRESHED'), true, 'original status can refresh')
  MiniTest.expect.equality(type(wait_menu()), 'table', 'original status can open a menu')
  child.lua("M.press({ 'q' })")
  MiniTest.expect.equality(child.lua_get('VIEW.current().key == STATUS.key'), true)
  child.lua(string.format([[
    VIEW.focus(%q)
    SECOND_STATUS = VIEW.current()
    SECOND_BEFORE = SECOND_STATUS.version
    M.press({ 'g', 'r' })
    SECOND_REFRESHED = vim.wait(5000, function() return SECOND_STATUS.version > SECOND_BEFORE end, 10)
  ]], second.key))
  MiniTest.expect.equality(
    child.lua_get('SECOND_REFRESHED'), true, 'other repository still refreshes'
  )
end

T['real-file edit documents share identity and keymap queues without scratch-buffer semantics'] = function()
  setup_child()
  child.lua(string.format([[
    local edit = require('magit-lsp.edit')
    local path = %q .. '/.git/COMMIT_EDITMSG'
    vim.fn.writefile({ 'original message' }, path)
    ANSWERS = {}
    VIEW.invoke = function(command, opts, key)
      ANSWERS[#ANSWERS + 1] = { command, opts.args.editId, key }
    end
    local function open(id)
      VIEW.apply_keymap({ viewId = id, scope = 'commit-message', mode = 'interactive',
        capture = false, bindings = {
          { command = 'common.confirm', keys = { 'Z Z' }, modes = { 'normal' } },
          { command = 'common.cancel', keys = { 'Z Q' }, modes = { 'normal' } },
        },
      }, STATUS.client_id)
      return edit.open({ editId = id, path = path, kind = 'commit-message' },
        { client_id = STATUS.client_id, root = STATUS.root })
    end
    DOC = open('edit-confirm')
    local maps = {}
    for _, map in ipairs(vim.api.nvim_buf_get_keymap(DOC.buf, 'n')) do maps[map.lhs] = true end
    DOCUMENT = { name = vim.api.nvim_buf_get_name(DOC.buf),
      buftype = vim.bo[DOC.buf].buftype, filetype = vim.bo[DOC.buf].filetype,
      editable = vim.bo[DOC.buf].modifiable, base = DOC.base, current = VIEW.current() == DOC,
      maps = maps, server_views = #VIEW.order,
      debug_views = #require('magit-lsp.debug').order,
      pending_empty = next(VIEW.pending_keymap) == nil }
    vim.api.nvim_buf_set_lines(DOC.buf, 0, -1, false, { 'saved message' })
    edit.confirm(DOC.key)
    SAVED = vim.fn.readfile(path)
    CONFIRMED = VIEW.views[DOC.key] == nil and VIEW.current() == STATUS
    DOC = open('edit-cancel')
    vim.api.nvim_buf_set_lines(DOC.buf, 0, -1, false, { 'discard me' })
    edit.cancel(DOC.key)
    CANCELLED = vim.fn.readfile(path)
    DOC = open('edit-wipe')
    vim.api.nvim_buf_delete(DOC.buf, { force = true })
    WIPED = VIEW.views[DOC.key] == nil

    FLOAT = VIEW.open({ id = 'float-test', kind = 'unrelated-kind',
      uri = 'magit://test/float', placement = 'float', mode = 'interactive' },
      { client_id = STATUS.client_id, root = STATUS.root })
    VIEW.apply_render({ viewId = FLOAT.id, version = 1, mode = 'interactive',
      text = { 'one', '  two', 'three' } }, STATUS.client_id)
    FLOAT_STATE = { height = vim.api.nvim_win_get_height(FLOAT.win),
      relative = vim.api.nvim_win_get_config(FLOAT.win).relative, text = FLOAT.lines }
    VIEW.close(FLOAT.key, true)
  ]], repo))
  MiniTest.expect.equality(child.lua_get('DOCUMENT'), {
    name = repo .. '/.git/COMMIT_EDITMSG', buftype = '', filetype = 'gitcommit',
    editable = true, base = child.lua_get('STATUS.key'), current = true,
    maps = { ZZ = true, ZQ = true }, server_views = 1, debug_views = 1, pending_empty = true,
  })
  MiniTest.expect.equality(child.lua_get('SAVED'), { 'saved message' })
  MiniTest.expect.equality(child.lua_get('CANCELLED'), { 'saved message' })
  MiniTest.expect.equality(child.lua_get('CONFIRMED and WIPED'), true)
  local base = child.lua_get('STATUS.key')
  MiniTest.expect.equality(child.lua_get('ANSWERS'), {
    { 'common.confirm', 'edit-confirm', base },
    { 'common.cancel', 'edit-cancel', base },
    { 'common.cancel', 'edit-wipe', base },
  })
  MiniTest.expect.equality(child.lua_get('FLOAT_STATE'), {
    height = 3, relative = 'editor', text = { 'one', '  two', 'three' },
  })
end

T['descriptor queues, deferred renders and resync preserve client identity and text mode'] = function()
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
          assert(method == 'view/resync')
          RESYNCS[#RESYNCS + 1] = { client_id = id, callback = callback }
        end,
      }
    end
    vim.lsp.get_client_by_id = function(id) return clients[id] end
    vim.lsp.buf_is_attached = function() return true end
    vim.lsp.semantic_tokens.force_refresh = function() end
    vim.lsp.buf_request_sync = function() return { [101] = { result = {} } } end
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
    debug_api.request_started()
    VIEW.apply_render({ viewId = 'same', version = 3, baseVersion = 2, mode = 'text',
      edits = { { startLine = 0, endLine = 1, lines = { 'a deferred' } } },
    }, 101)
    VIEW.apply_render({ viewId = 'same', version = 2, baseVersion = 1, mode = 'interactive',
      edits = { { startLine = 0, endLine = 1, lines = { 'b deferred' } } },
    }, 202)
    HELD = { A.version, B.version, #ACKS }
    debug_api.request_finished()
    VIEW.sync(A.buf, A.client_id, 'probe', {})
    DEFERRED = { a = A.lines, b = B.lines, versions = { A.version, B.version },
      editable = { vim.bo[A.buf].modifiable, vim.bo[B.buf].modifiable },
      empty = next(VIEW.deferred_render) == nil }

    VIEW.apply_render({ viewId = 'same', version = 99, baseVersion = 0, mode = 'interactive',
      edits = { { startLine = 0, endLine = 1, lines = { 'WRONG BASE' } } },
    }, 101)
    MISMATCH = { mode = A.mode, editable = vim.bo[A.buf].modifiable,
      text = A.lines, version = A.version, acks = #ACKS,
      resyncs = #RESYNCS, client_id = RESYNCS[1].client_id }
    RESYNCS[1].callback(nil, { viewId = 'same', version = 4,
      mode = 'interactive', text = { 'fresh server text' } })
    RECOVERED = { mode = A.mode, editable = vim.bo[A.buf].modifiable,
      text = A.lines, version = A.version, other = B.lines }
    VIEW.close(A.key)
    INDEPENDENT_CLOSE = VIEW.views[A.key] == nil and VIEW.views[B.key] == B
    vim.api.nvim_buf_delete(B.buf, { force = true })
    CLEAN = vim.wait(1000, function() return debug_api.idle() end, 10)
      and #VIEW.order == 0 and #debug_api.order == 0
  ]])
  MiniTest.expect.equality(child.lua_get('EARLY'), {
    a = { '  alpha', '  replacement', '' }, b = { 'second' }, versions = { 2, 1 },
    editable = { true, false }, keymaps = { 'new-kind', 'new-kind' },
    pending_empty = true, distinct = true,
  })
  MiniTest.expect.equality(child.lua_get('HELD'), { 2, 1, 3 })
  MiniTest.expect.equality(child.lua_get('DEFERRED'), {
    a = { 'a deferred', '  replacement', '' }, b = { 'b deferred' }, versions = { 3, 2 },
    editable = { true, false }, empty = true,
  })
  MiniTest.expect.equality(child.lua_get('MISMATCH'), {
    mode = 'text', editable = true, text = { 'a deferred', '  replacement', '' },
    version = 3, acks = 5, resyncs = 1, client_id = 101,
  })
  MiniTest.expect.equality(child.lua_get('RECOVERED'), {
    mode = 'interactive', editable = false, text = { 'fresh server text' },
    version = 4, other = { 'b deferred' },
  })
  local acks = child.lua_get('ACKS')
  table.sort(acks, function(a, b)
    return a[1] < b[1] or (a[1] == b[1] and a[3] < b[3])
  end)
  MiniTest.expect.equality(acks, {
    { 101, 'same', 1 }, { 101, 'same', 2 }, { 101, 'same', 3 }, { 101, 'same', 4 },
    { 202, 'same', 1 }, { 202, 'same', 2 },
  })
  MiniTest.expect.equality(child.lua_get('INDEPENDENT_CLOSE and CLEAN'), true)
end

return T
