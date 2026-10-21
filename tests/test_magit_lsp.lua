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

local function setup_child()
  child.lua(string.format("M = require('magit-lsp'); M.setup({ root_dir = %s })", vim.inspect(repo)))
  child.lua([[
    HELPERS = {
      has_line = function(s) return vim.tbl_contains(vim.api.nvim_buf_get_lines(M.buf, 0, -1, false), s) end,
      find_line = function(marker, text, section)
        for i, e in ipairs(M.map) do
          if e.kind == 'line' and e.marker == marker and e.text == text then
            if not section or e.section == section then
              return i
            end
          end
        end
        return nil
      end,
      find_first = function(kind, section)
        for i, e in ipairs(M.map) do
          if e.kind == kind and (not section or e.section == section) then
            return i
          end
        end
        return nil
      end,
      has_status_line = function(section, marker, text)
        local files = M.status[section] or {}
        for _, f in ipairs(files) do
          for _, h in ipairs(f.hunks) do
            for _, l in ipairs(h.lines) do
              if l.marker == marker and l.text == text then
                return true
              end
            end
          end
        end
        return false
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

T['creates virtual buffer and attaches LSP'] = function()
  setup_child()
  local buftype = child.lua_get("vim.api.nvim_get_option_value('buftype', { buf = M.buf })")
  MiniTest.expect.equality(buftype, 'nofile')

  local bufname = child.lua_get('vim.api.nvim_buf_get_name(M.buf)')
  MiniTest.expect.equality(bufname, 'vcs://status')

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

T['staging a hunk moves the file to staged section'] = function()
  setup_child()
  local hunk_id = child.lua_get('M.status.unstaged[1].hunks[1].id')
  MiniTest.expect.equality(type(hunk_id), 'string', 'hunk id')
  child.lua(string.format("M.stage_hunk('f.txt', %q)", hunk_id))
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 0)
  assert_line('Staged changes (1)')
  assert_line('Unstaged changes (0)')
end

T['staging a visual line range stages only those lines'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'line index for +NEW line in unstaged section')
  child.lua('S = M.stage_line_range(' .. target .. ', ' .. target .. ')')
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)

  local staged_new = child.lua_get("HELPERS.has_status_line('staged', '+', 'NEW line')")
  MiniTest.expect.equality(staged_new, true)

  local unstaged_change = child.lua_get("HELPERS.has_status_line('unstaged', '+', 'line3 CHANGED')")
  MiniTest.expect.equality(unstaged_change, true)

  local unstaged_del = child.lua_get("HELPERS.has_status_line('unstaged', '-', 'line8')")
  MiniTest.expect.equality(unstaged_del, true)
end

T['shows unstaged diff for a file'] = function()
  setup_child()
  child.lua("D = M.diff('f.txt', false)")
  local diff = child.lua_get('D.diff')
  MiniTest.expect.equality(type(diff), 'string')
  MiniTest.expect.equality(diff:find('-line3', 1, true) ~= nil, true)
  MiniTest.expect.equality(diff:find('+line3 CHANGED', 1, true) ~= nil, true)
  MiniTest.expect.equality(diff:find('+NEW line', 1, true) ~= nil, true)
end

T['unstaging a hunk reverts the file to unstaged'] = function()
  setup_child()
  local hunk_id = child.lua_get('M.status.unstaged[1].hunks[1].id')
  child.lua(string.format("M.stage_hunk('f.txt', %q)", hunk_id))
  local staged_id = child.lua_get('M.status.staged[1].hunks[1].id')
  child.lua(string.format("M.unstage_hunk('f.txt', %q)", staged_id))
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 0)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 1)
end

T['stage_at stages the hunk under the cursor'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('hunk', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'hunk line index in unstaged section')
  child.lua('vim.api.nvim_win_set_cursor(0, {' .. target .. ', 0}); S = M.stage_at()')
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 0)
end

T['stage_at over a change line stages the whole hunk'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.lua('vim.api.nvim_win_set_cursor(0, {' .. target .. ', 0}); S = M.stage_at()')
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1, 'staged files')
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 0, 'unstaged files')
end

T['S stages all changes'] = function()
  setup_child()
  child.lua("S = M.stage_all()")
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 0)
end

T['U unstages all changes'] = function()
  setup_child()
  child.lua("M.stage_all()")
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1)
  child.lua("M.unstage_all()")
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 0)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 1)
end

T['U unstages all without a commit'] = function()
  repo = make_unborn_repo()
  setup_child()
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1)
  child.lua("S = M.unstage_all()")
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 0)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 0)
  MiniTest.expect.equality(child.lua_get('#M.status.untracked'), 1)
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
  child.lua('R = M.stage_all()')
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

T['visual mode s stages the selected lines'] = function()
  setup_child()
  local target = child.lua_get("HELPERS.find_line('+', 'NEW line', 'unstaged')")
  MiniTest.expect.equality(type(target), 'number', 'change line index')
  child.api.nvim_win_set_cursor(0, { target, 0 })
  child.type_keys(50, 'V', 's')

  local staged = false
  for _ = 1, 20 do
    staged = child.lua_get("HELPERS.has_status_line('staged', '+', 'NEW line')")
    if staged then
      break
    end
    vim.loop.sleep(50)
  end
  MiniTest.expect.equality(staged, true)
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

  local hunk_id = child.lua_get('M.status.unstaged[1].hunks[1].id')
  child.lua(string.format("M.stage_hunk('f.txt', %q)", hunk_id))
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1)

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

T['enables LSP folding and serves fold ranges'] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get('vim.wo.foldmethod'), 'expr')
  MiniTest.expect.equality(child.lua_get('vim.wo.foldexpr'), 'v:lua.vim.lsp.foldexpr()')

  local count = child.lua_get([[
    (function()
      local r = vim.lsp.buf_request_sync(M.buf, 'textDocument/foldingRange', {
        textDocument = { uri = vim.uri_from_bufnr(M.buf) },
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

  local hunk_id = child.lua_get('M.status.unstaged[1].hunks[1].id')
  child.lua(string.format("M.stage_hunk('f.txt', %q)", hunk_id))
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 0)
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

T['navigation targets come from the LSP'] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get('type(M.targets())'), 'table')
  local has_section = child.lua_get([[
    (function()
      for _, t in ipairs(M.targets()) do
        if t.kind == 'section' then
          return true
        end
      end
      return false
    end)()
  ]])
  MiniTest.expect.equality(has_section, true)

  child.lua('vim.api.nvim_win_set_cursor(0, { 1, 0 }); J = M.jump("file", 1)')
  MiniTest.expect.equality(child.lua_get('type(J)'), 'number')
  MiniTest.expect.equality(child.lua_get('vim.fn.line(".")'), child.lua_get('J'))
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
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 1)
  MiniTest.expect.equality(child.lua_get('#M.status.untracked'), 2)
end

T['stage_at on an untracked file stages it'] = function()
  repo = make_untracked_repo()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('file', 'untracked')")
  MiniTest.expect.equality(type(target), 'number', 'untracked file line index')
  child.lua('vim.api.nvim_win_set_cursor(0, {' .. target .. ', 0}); S = M.stage_at()')
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)
  MiniTest.expect.equality(child.lua_get('#M.status.untracked'), 1)
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 1)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 1)
  assert_line('Untracked files (1)')
  assert_line('  A a b.txt')
end

T['stage_at on the untracked section stages all untracked'] = function()
  repo = make_untracked_repo()
  setup_child()
  local target = child.lua_get("HELPERS.find_first('section', 'untracked')")
  MiniTest.expect.equality(type(target), 'number', 'untracked section line index')
  child.lua('vim.api.nvim_win_set_cursor(0, {' .. target .. ', 0}); S = M.stage_at()')
  MiniTest.expect.equality(child.lua_get('S ~= nil'), true)
  MiniTest.expect.equality(child.lua_get('#M.status.untracked'), 0)
  MiniTest.expect.equality(child.lua_get('#M.status.staged'), 2)
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 1)
  assert_line('Untracked files (0)')
  assert_line('  A new.txt')
end

T['unstage_at on an untracked file reports nothing to unstage'] = function()
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
  child.lua('vim.api.nvim_win_set_cursor(0, {' .. target .. ', 0}); U = M.unstage_at()')
  MiniTest.expect.equality(child.lua_get('U == nil'), true)
  MiniTest.expect.equality(child.lua_get('#NOTIFIES'), 1)
  MiniTest.expect.equality(child.lua_get('NOTIFIES[1].msg'), 'Nothing to unstage here')
end

return T
