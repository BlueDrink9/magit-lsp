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

local function make_repo(with_changes)
  if with_changes == nil then
    with_changes = true
  end
  local path = project .. '/build/test-repos/fixture'
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

local function make_unborn_repo()
  local path = project .. '/build/test-repos/unborn'
  sys({ 'rm', '-rf', path })
  vim.fn.mkdir(path, 'p')
  sys({ 'git', 'init', '-q', '-b', 'main', path })
  sys({ 'git', '-C', path, 'config', 'user.email', 't@example.com' })
  sys({ 'git', '-C', path, 'config', 'user.name', 'tester' })
  vim.fn.writefile({ 'hello' }, path .. '/f.txt')
  sys({ 'git', '-C', path, 'add', 'f.txt' })
  return path
end

--- A repo with a tracked modification plus two untracked files, one spaced.
local function make_untracked_repo()
  local path = make_repo()
  vim.fn.writefile({ 'untracked' }, path .. '/new.txt')
  vim.fn.writefile({ 'untracked' }, path .. '/a b.txt')
  return path
end

local function setup_child(opts)
  local init = { root_dir = repo }
  for key, value in pairs(opts or {}) do
    init[key] = value
  end
  child.lua(
    string.format("M = require('magit-lsp'); M.setup(%s)", vim.inspect(init))
  )
  child.lua([[
    HELPERS = {
      lines = function()
        return vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)
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
      section_range = function(section)
        local headers = {
          staged = 'Staged changes (',
          unstaged = 'Unstaged changes (',
          untracked = 'Untracked files (',
        }
        local wanted = headers[section]
        local lines = HELPERS.lines()
        local first
        for i, line in ipairs(lines) do
          if vim.startswith(line, wanted) then
            first = i
            break
          end
        end
        if not first then
          return nil
        end
        local last = #lines
        for i = first + 1, #lines do
          local line = lines[i]
          if
            vim.startswith(line, 'Staged changes (')
            or vim.startswith(line, 'Unstaged changes (')
            or vim.startswith(line, 'Untracked files (')
          then
            last = i - 1
            break
          end
        end
        return first, last
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
        local first, last = HELPERS.section_range(section)
        if not first then
          return nil
        end
        if kind == 'section' then
          return first
        end
        local lines = HELPERS.lines()
        for i = first, last do
          local line = lines[i]
          if kind == 'hunk' and vim.startswith(line, '    @@') then
            return i
          end
          if kind == 'file' and line:match('^  %S ') then
            return i
          end
        end
        return nil
      end,
      token_at = function(row, col)
        local tokens = vim.lsp.semantic_tokens.get_at_pos(M.buf, row, col)
        return tokens and tokens[1] or nil
      end,
      token_type_on = function(text, col)
        for i, line in ipairs(M.lines) do
          if line == text then
            local token = HELPERS.token_at(i - 1, col or 0)
            return token and token.type or nil
          end
        end
        return nil
      end,
      wait_tokens = function()
        return vim.wait(5000, function()
          for i = 0, #M.lines - 1 do
            if HELPERS.token_at(i, 0) then
              return true
            end
          end
          return false
        end, 20)
      end,
      legend = function()
        local client = vim.lsp.get_client_by_id(M.client_id)
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
        local client = vim.lsp.get_client_by_id(M.client_id)
        local done, result
        client:request('view/resync', { viewId = M.view_id }, function(err, value)
          result = value
          done = true
        end, M.buf)
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
        for _, map in ipairs(vim.api.nvim_buf_get_keymap(M.buf, mode or 'n')) do
          out[map.lhs] = true
        end
        return out
      end,
      expected_keymap = function(notice, mode)
        mode = mode or 'n'
        local keys = require('magit-lsp.keys')
        local want = mode == 'n' and 'normal' or 'visual'
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
        local visit = not visit_implemented and notice.scope == 'status'
        if visit then
          out[HELPERS.normalize_lhs('<CR>', mode)] = (mode == 'n') or nil
        end
        if mode == 'n' then
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
        local want = HELPERS.expected_keymap(M.last_keymap, mode or 'n')
        local got = HELPERS.applied_lhs(mode or 'n')
        local missing, extra = HELPERS.diff_keys(got, want)
        return { missing = missing, extra = extra }
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

-- Type keys that the read-only buffer should refuse. `type_keys()` raises when
-- `v:errmsg` is set, so accept only the expected native E21 and never a Lua
-- traceback.
local function type_edit(keys)
  local ok, err = pcall(child.type_keys, 50, keys)
  vim.loop.sleep(50)
  local errmsg = child.v.errmsg
  child.v.errmsg = ''
  if not ok then
    MiniTest.expect.equality(
      tostring(err):find('E21', 1, true) ~= nil,
      true,
      'unexpected error: ' .. tostring(err)
    )
  elseif errmsg ~= '' then
    MiniTest.expect.equality(errmsg:find('E21', 1, true) ~= nil, true, 'unexpected error: ' .. errmsg)
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
        'if M and M.client_id then local c = vim.lsp.get_client_by_id(M.client_id); if c then c:stop(true) end end'
      )
      child.stop()
    end,
  },
})

T['creates a server-named virtual buffer and attaches LSP'] = function()
  setup_child()
  local buftype = child.lua_get("vim.api.nvim_get_option_value('buftype', { buf = M.buf })")
  MiniTest.expect.equality(buftype, 'nofile')

  local bufname = child.lua_get('vim.api.nvim_buf_get_name(M.buf)')
  MiniTest.expect.equality(bufname, 'magit://status' .. repo)

  local attached = child.lua_get('vim.lsp.buf_is_attached(M.buf, M.client_id)')
  MiniTest.expect.equality(attached, true)
end

T['renders status with unstaged changes'] = function()
  setup_child()
  assert_line('Head: main')
  assert_line('Staged changes (0)')
  assert_line('Unstaged changes (1)')
  assert_line('  M f.txt')
  assert_line('    -line3')
  assert_line('    +line3 CHANGED')
  assert_line('    +NEW line')
  assert_line('    -line8')
end

T['renders a non-blank buffer when the tree is clean'] = function()
  repo = make_repo(false)
  setup_child()
  assert_line('Head: main')
  assert_line('Staged changes (0)')
  assert_line('Unstaged changes (0)')
  assert_line('Working tree clean')
end

T['staging a hunk moves the file to the staged section'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  MiniTest.expect.equality(invoke_at(target, 'status.stage'), true)

  assert_line('Staged changes (1)')
  assert_line('Unstaged changes (0)')
  MiniTest.expect.equality(git_out({ 'diff', '--cached', '-U3', '--', 'f.txt' }):find('+NEW line', 1, true) ~= nil, true)
end

T['staging a visual line range stages only those lines'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, 'V', 's')

  local staged = false
  for _ = 1, 40 do
    staged = child.lua_get("HELPERS.has_line('    +NEW line')")
    if staged then
      break
    end
    vim.loop.sleep(50)
  end
  MiniTest.expect.equality(staged, true)

  local cached = git_out({ 'diff', '--cached', '-U3', '--', 'f.txt' })
  MiniTest.expect.equality(cached:find('+NEW line', 1, true) ~= nil, true, cached)
  MiniTest.expect.equality(cached:find('line3 CHANGED', 1, true), nil, cached)
  MiniTest.expect.equality(git_out({ 'diff', '-U3', '--', 'f.txt' }):find('line3 CHANGED', 1, true) ~= nil, true)
end

T['unstaging a hunk reverts the file to unstaged'] = function()
  setup_child()
  child.lua("M.invoke('status.stage-all')")
  MiniTest.expect.equality(git_out({ 'diff', '--cached', '--', 'f.txt' }):find('NEW line', 1, true) ~= nil, true)

  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'staged')")
  MiniTest.expect.equality(type(target), 'number', 'staged change line index')
  MiniTest.expect.equality(invoke_at(target, 'status.unstage'), true)

  assert_line('Staged changes (0)')
  assert_line('Unstaged changes (1)')
  MiniTest.expect.equality(git_out({ 'diff', '--cached', '--', 'f.txt' }), '')
end

T['staging a file header stages the whole file'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'unstaged file index')
  MiniTest.expect.equality(invoke_at(target, 'status.stage'), true)
  assert_line('Staged changes (1)')
  assert_line('Unstaged changes (0)')

  local staged = child.lua_get("HELPERS.find_first('file', 'staged')")
  MiniTest.expect.equality(type(staged), 'number', 'staged file index')
  MiniTest.expect.equality(invoke_at(staged, 'status.unstage'), true)
  assert_line('Staged changes (0)')
  assert_line('Unstaged changes (1)')
  MiniTest.expect.equality(git_out({ 'diff', '--cached', '--', 'f.txt' }), '')
end

T['staging a hunk header stages the whole hunk'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('hunk', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'hunk header index')
  MiniTest.expect.equality(invoke_at(target, 'status.stage'), true)
  assert_line('Staged changes (1)')
  assert_line('Unstaged changes (0)')
end

T['S stages all changes'] = function()
  setup_child()
  child.lua("S = M.invoke('status.stage-all')")
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)
  assert_line('Staged changes (1)')
  assert_line('Unstaged changes (0)')
end

T['U unstages all changes'] = function()
  setup_child()
  child.lua("M.invoke('status.stage-all')")
  assert_line('Staged changes (1)')
  child.lua("M.invoke('status.unstage-all')")
  assert_line('Staged changes (0)')
  assert_line('Unstaged changes (1)')
end

T['U unstages all without a commit'] = function()
  repo = make_unborn_repo()
  setup_child()
  assert_line('Staged changes (1)')
  child.lua("S = M.invoke('status.unstage-all')")
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)
  assert_line('Staged changes (0)')
  assert_line('Unstaged changes (0)')
  assert_line('Untracked files (1)')
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
    child.lua_get("vim.api.nvim_get_option_value('filetype', { buf = M.buf })"),
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

  child.lua("vim.cmd('colorscheme vim')")
  MiniTest.expect.equality(child.lua_get('HELPERS.missing_links()'), {})

  local switched_added = child.lua_get("HELPERS.resolved_fg('magit.diff.added')")
  local switched_removed = child.lua_get("HELPERS.resolved_fg('magit.diff.removed')")
  MiniTest.expect.equality(switched_added ~= switched_removed, true, 'added and removed differ')
  MiniTest.expect.equality(switched_added ~= added, true, 'the scheme changed the colour')
end

T['tokens are refreshed after a mutation'] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get('HELPERS.wait_tokens()'), true, 'semantic tokens arrive')

  child.lua("M.invoke('status.stage-all')")
  assert_line('Staged changes (1)')

  local staged = child.lua_get([[
    (function()
      return vim.wait(5000, function()
        for i, line in ipairs(M.lines) do
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
      local r = vim.lsp.buf_request_sync(M.buf, 'textDocument/foldingRange', {
        textDocument = { uri = M.uri },
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
  local before = child.lua_get('vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)')

  local function opt(name)
    return child.lua_get(string.format("vim.api.nvim_get_option_value(%q, { buf = M.buf })", name))
  end
  MiniTest.expect.equality(opt('buftype'), 'nofile')
  MiniTest.expect.equality(opt('bufhidden'), 'hide')
  MiniTest.expect.equality(opt('swapfile'), false)
  MiniTest.expect.equality(opt('modifiable'), false)
  MiniTest.expect.equality(opt('readonly'), true)
  MiniTest.expect.equality(opt('undolevels'), -1)
  MiniTest.expect.equality(opt('modified'), false)

  -- `u` is mapped to unstage; the cursor sits on the header, so it is a no-op.
  type_edit('dd')
  type_edit('x')
  type_edit('ciw<Esc>')
  type_edit('u')

  local ok, err = pcall(child.cmd, 's/a/b/')
  MiniTest.expect.equality(ok, false, 'substitute should be refused')
  MiniTest.expect.equality(
    tostring(err):find('E21', 1, true) ~= nil,
    true,
    'substitute should fail with E21: ' .. tostring(err)
  )
  child.v.errmsg = ''

  local after = child.lua_get('vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)')
  MiniTest.expect.equality(after, before)
  MiniTest.expect.equality(opt('modified'), false)
end

T['refresh and mutations still work after blocked edits'] = function()
  setup_child()
  type_edit('dd')
  type_edit('x')

  local path = repo .. '/f.txt'
  local lines = vim.fn.readfile(path)
  table.insert(lines, 'line11')
  vim.fn.writefile(lines, path)

  child.lua('M.refresh()')
  assert_line('    +line11')
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('modified', { buf = M.buf })"),
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
  local before = child.lua_get('vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)')

  local ok, err = pcall(child.type_keys, 100, ':write<CR>')
  MiniTest.expect.equality(ok, true, 'write must not raise: ' .. tostring(err))
  vim.loop.sleep(100)
  MiniTest.expect.equality(child.v.errmsg, '', 'write must not produce a Vim error')
  child.v.errmsg = ''

  MiniTest.expect.equality(child.lua_get('#HINTS'), 1)
  local msg = child.lua_get('HINTS[1].msg')
  MiniTest.expect.equality(type(msg), 'string')
  MiniTest.expect.equality(msg:find('Read-only view', 1, true) ~= nil, true, msg)
  MiniTest.expect.equality(child.lua_get('HINTS[1].level'), vim.log.levels.INFO)

  child.type_keys(100, ':write<CR>')
  vim.loop.sleep(100)
  MiniTest.expect.equality(child.lua_get('#HINTS'), 1, 'the hint is shown once per session')

  local after = child.lua_get('vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)')
  MiniTest.expect.equality(after, before)
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('modified', { buf = M.buf })"),
    false
  )
  MiniTest.expect.equality(
    child.lua_get("vim.api.nvim_get_option_value('modifiable', { buf = M.buf })"),
    false
  )
end

T['navigation jumps follow document symbols'] = function()
  setup_child()
  local symbols = child.lua_get([[
    (function()
      local r = vim.lsp.buf_request_sync(M.buf, 'textDocument/documentSymbol', {
        textDocument = { uri = M.uri },
      }, 5000)
      for _, res in pairs(r or {}) do
        return res.result or {}
      end
      return {}
    end)()
  ]])
  MiniTest.expect.equality(#symbols > 0, true, 'documentSymbol returns items')

  local staged_line, unstaged_section_line, unstaged_line, hunk_line
  for i, line in ipairs(child.lua_get('M.lines')) do
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

  local opened = false
  for _ = 1, 100 do
    local name = child.lua_get('vim.api.nvim_buf_get_name(0)')
    if name:match('f%.txt$') then
      opened = true
      break
    end
    vim.loop.sleep(50)
  end
  MiniTest.expect.equality(opened, true, 'the worktree file opened')
  MiniTest.expect.equality(child.lua_get('vim.fn.line(".")'), 3)
end

T['renders untracked files in their own section'] = function()
  repo = make_untracked_repo()
  setup_child()
  MiniTest.expect.equality(child.lua_get('M.lines'), {
    'Head: main',
    '',
    'Staged changes (0)',
    '',
    'Unstaged changes (1)',
    '  M f.txt',
    '    @@ -1,10 +1,10 @@',
    '     line1',
    '     line2',
    '    -line3',
    '    +line3 CHANGED',
    '     line4',
    '     line5',
    '    +NEW line',
    '     line6',
    '     line7',
    '    -line8',
    '     line9',
    '     line10',
    '',
    'Untracked files (2)',
    '  ? a b.txt',
    '  ? new.txt',
  })
  MiniTest.expect.equality(git_out({ 'status', '--porcelain', '--', 'new.txt' }):find('?? new.txt', 1, true) ~= nil, true)
end

T['staging an untracked file stages just that file'] = function()
  repo = make_untracked_repo()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'untracked')")
  MiniTest.expect.equality(type(target), 'number', 'untracked file line index')
  MiniTest.expect.equality(invoke_at(target, 'status.stage'), true)
  assert_line('Untracked files (1)')
  assert_line('Staged changes (1)')
  assert_line('  A a b.txt')
  MiniTest.expect.equality(
    git_out({ 'status', '--porcelain', '-z', '--', 'a b.txt' }):find('A  a b.txt', 1, true) ~= nil,
    true
  )
end

T['staging the untracked section stages all untracked'] = function()
  repo = make_untracked_repo()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('section', 'untracked')")
  MiniTest.expect.equality(type(target), 'number', 'untracked section line index')
  MiniTest.expect.equality(invoke_at(target, 'status.stage'), true)
  assert_line('Untracked files (0)')
  assert_line('Staged changes (2)')
  assert_line('  A new.txt')
  MiniTest.expect.equality(git_out({ 'status', '--porcelain', '--', 'new.txt' }):find('A  new.txt', 1, true) ~= nil, true)
end

T['unstaging an untracked file reports nothing to unstage'] = function()
  repo = make_untracked_repo()
  setup_child()
  child.lua([[
    NOTIFIES = {}
    vim.notify = function(msg, level)
      table.insert(NOTIFIES, { msg = msg, level = level })
    end
  ]])
  local target = child.lua_get("HELPERS.find_first('file', 'untracked')")
  MiniTest.expect.equality(type(target), 'number', 'untracked file line index')
  invoke_at(target, 'status.unstage')
  MiniTest.expect.equality(child.lua_get('#NOTIFIES'), 1)
  MiniTest.expect.equality(child.lua_get('NOTIFIES[1].msg'), 'Nothing to unstage here')
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
  MiniTest.expect.equality(child.lua_get('M.version'), resync.version)
  assert_line('Staged changes (1)')
end

T['a lost update triggers a full resync'] = function()
  setup_child()
  -- Pretend the client missed a render: the next edits no longer match.
  child.lua('M.version = M.version - 1')
  child.lua("M.invoke('status.stage-all')")

  local buffer = child.lua_get('HELPERS.lines()')
  local resync = child.lua_get('HELPERS.resync()')
  MiniTest.expect.equality(buffer, resync.text)
  MiniTest.expect.equality(child.lua_get('M.version'), resync.version)
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
    M.apply_keymap({
      viewId = M.view_id,
      scope = 'status',
      version = M.version,
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
    })
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

  local staged = false
  for _ = 1, 40 do
    staged = child.lua_get("HELPERS.has_line('Staged changes (1)')")
    if staged then
      break
    end
    vim.loop.sleep(50)
  end
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
  local refreshed = false
  for _ = 1, 40 do
    refreshed = child.lua_get("HELPERS.has_line('    +line11')")
    if refreshed then
      break
    end
    vim.loop.sleep(50)
  end
  MiniTest.expect.equality(refreshed, true)
end

T['Tab toggles a file section through the server'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'file line index')
  local before = child.lua_get('vim.api.nvim_buf_line_count(M.buf)')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<Tab>')

  local collapsed = false
  for _ = 1, 40 do
    collapsed = child.lua_get('vim.api.nvim_buf_line_count(M.buf)') < before
    if collapsed then
      break
    end
    vim.loop.sleep(50)
  end
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
  local before = child.lua_get('vim.api.nvim_buf_line_count(M.buf)')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<F2>')

  local collapsed = false
  for _ = 1, 40 do
    collapsed = child.lua_get('vim.api.nvim_buf_line_count(M.buf)') < before
    if collapsed then
      break
    end
    vim.loop.sleep(50)
  end
  MiniTest.expect.equality(collapsed, true, 'the mirrored fold key collapsed the file')
end

T['a user Tab fold mapping toggles the section'] = function()
  child.cmd('nnoremap <Tab> za')
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  local before = child.lua_get('vim.api.nvim_buf_line_count(M.buf)')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<Tab>')

  local collapsed = false
  for _ = 1, 40 do
    collapsed = child.lua_get('vim.api.nvim_buf_line_count(M.buf)') < before
    if collapsed then
      break
    end
    vim.loop.sleep(50)
  end
  MiniTest.expect.equality(collapsed, true, 'the user Tab mapping toggled the file')
end

T['fold_keys mirrors a Lua-function fold mapping'] = function()
  setup_child({ fold_keys = { ['<F3>'] = 'common.toggle-section' } })
  MiniTest.expect.equality(child.lua_get("HELPERS.applied_lhs('n')['<F3>'] ~= nil"), true)

  local target = child.lua_get("HELPERS.find_first('file', 'unstaged')")
  local before = child.lua_get('vim.api.nvim_buf_line_count(M.buf)')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, '<F3>')

  local collapsed = false
  for _ = 1, 40 do
    collapsed = child.lua_get('vim.api.nvim_buf_line_count(M.buf)') < before
    if collapsed then
      break
    end
    vim.loop.sleep(50)
  end
  MiniTest.expect.equality(collapsed, true, 'the configured fold key collapsed the file')
end

T['capture mode forwards printable keys and shows the server hint'] = function()
  setup_child()
  child.lua([[
    M.apply_keymap({
      viewId = M.view_id,
      scope = 'menu',
      version = M.version,
      capture = true,
      bindings = {},
    })
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
  local before = child.lua_get('vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)')
  child.type_keys(50, 'Q')

  MiniTest.expect.equality(child.lua_get('SENT'), { 'Q' })
  MiniTest.expect.equality(child.lua_get('#NOTIFIES'), 1)
  local msg = child.lua_get('NOTIFIES[1].msg')
  MiniTest.expect.equality(msg:find('Unbound key: Q', 1, true) ~= nil, true, msg)
  local after = child.lua_get('vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)')
  MiniTest.expect.equality(after, before)
end

T['visit_key overrides the planned common.visit binding'] = function()
  setup_child({ visit_key = '<F4>' })
  local target = child.lua_get("HELPERS.find_line('+', 'line3 CHANGED', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { target, 0 })

  -- `common.visit` is Planned, so Enter must not leave the view.
  child.type_keys(50, '<CR>')
  vim.loop.sleep(100)
  MiniTest.expect.equality(
    child.lua_get('vim.api.nvim_buf_get_name(0)'):match('magit://') ~= nil,
    true,
    'Enter stays in the view until common.visit is implemented'
  )

  child.type_keys(50, '<F4>')
  local opened = false
  for _ = 1, 100 do
    local name = child.lua_get('vim.api.nvim_buf_get_name(0)')
    if name:match('f%.txt$') then
      opened = true
      break
    end
    vim.loop.sleep(50)
  end
  MiniTest.expect.equality(opened, true, 'the configured visit key opened the file')
  MiniTest.expect.equality(child.lua_get('vim.fn.line(".")'), 3)
end

return T
