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
      has_hl = function(group)
        for _, m in ipairs(vim.api.nvim_buf_get_extmarks(M.buf, M.ns, 0, -1, { details = true })) do
          if m[4].hl_group == group then
            return true
          end
        end
        return false
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
  child.lua("M.stage_hunk('f.txt', 0)")
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
  child.lua("M.stage_hunk('f.txt', 0)")
  child.lua("M.unstage_hunk('f.txt', 0)")
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
  MiniTest.expect.equality(child.lua_get('#M.status.unstaged'), 1)
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

T['colourises sections, files and diff lines'] = function()
  setup_child()
  MiniTest.expect.equality(child.lua_get("HELPERS.has_hl('MagitHeader')"), true)
  MiniTest.expect.equality(child.lua_get("HELPERS.has_hl('MagitSection')"), true)
  MiniTest.expect.equality(child.lua_get("HELPERS.has_hl('MagitFileModified')"), true)
  MiniTest.expect.equality(child.lua_get("HELPERS.has_hl('MagitDiffAdded')"), true)
  MiniTest.expect.equality(child.lua_get("HELPERS.has_hl('MagitDiffRemoved')"), true)
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

return T