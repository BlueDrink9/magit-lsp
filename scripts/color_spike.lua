-- IMM-8 spike: open the status view, dump its semantic tokens, and report the
-- resolved colours of every token group per colourscheme.
--
-- Run (from the project root):
--   nvim --headless --noplugin -u scripts/minimal_init.lua -l scripts/color_spike.lua
-- or:
--   make color-spike
--
-- Highlighting is owned by the server's semantic tokens, so this no longer
-- dumps client extmarks. `:TOhtml` skips the `nvim.lsp.semantic_tokens`
-- namespace, so the HTML is only a reference; the token table is the result.

local script_path = debug.getinfo(1, 'S').source:sub(2)
local script_dir = vim.fn.fnamemodify(script_path, ':p:h')
local project = vim.fn.fnamemodify(script_dir, ':h')

local out_dir = project .. '/build/test-output'
local repo = project .. '/build/test-repos/color-spike'

-- `tohtml` only emits colours from the `guifg`/`guibg` attributes, which are
-- populated when 'termguicolors' is on.
vim.o.termguicolors = true

local function sys(cmd)
  local out = vim.fn.system(cmd)
  if vim.v.shell_error ~= 0 then
    error('command failed: ' .. vim.inspect(cmd) .. '\n' .. out, 2)
  end
  return out
end

local function make_fixture()
  sys({ 'rm', '-rf', repo })
  vim.fn.mkdir(repo, 'p')
  sys({ 'git', 'init', '-q', '-b', 'main', repo })
  sys({ 'git', '-C', repo, 'config', 'user.email', 'spike@example.com' })
  sys({ 'git', '-C', repo, 'config', 'user.name', 'spike' })

  local lines = {}
  for i = 1, 10 do
    lines[i] = 'line' .. i
  end
  vim.fn.writefile(lines, repo .. '/f.txt')
  vim.fn.writefile({ 'a', 'b', 'c' }, repo .. '/s.txt')
  sys({ 'git', '-C', repo, 'add', '.' })
  sys({ 'git', '-C', repo, 'commit', '-q', '-m', 'init' })

  -- Staged change.
  vim.fn.writefile({ 'a', 'b STAGED', 'c' }, repo .. '/s.txt')
  sys({ 'git', '-C', repo, 'add', 's.txt' })

  -- Unstaged change: one modified, one added, one removed line.
  local changed = {}
  for i = 1, #lines do
    if i == 3 then
      changed[#changed + 1] = 'line3 CHANGED'
    elseif i == 8 then
      -- drop line8
    else
      changed[#changed + 1] = lines[i]
    end
    if i == 5 then
      changed[#changed + 1] = 'NEW line'
    end
  end
  vim.fn.writefile(changed, repo .. '/f.txt')

  -- Untracked file.
  vim.fn.writefile({ 'untracked' }, repo .. '/new.txt')
end

local function hex(n)
  if type(n) ~= 'number' then
    return '-'
  end
  return string.format('#%06x', n)
end

local function resolved_name(group)
  local id = vim.fn.hlID(group)
  if id == 0 then
    return group
  end
  local name = vim.fn.synIDattr(vim.fn.synIDtrans(id), 'name')
  return name ~= '' and name or group
end

--- Probe whether `tohtml` includes extmark highlights from a regular namespace
--- and from the LSP semantic-token namespace, and whether `hl_eol` is honoured.
---@return table
local function probe_tohtml()
  local tohtml = require('tohtml')
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, { 'AAAA BBBB', 'CCCC' })
  local win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, buf)

  vim.api.nvim_set_hl(0, 'SpikeRegular', { fg = 0xff0000 })
  vim.api.nvim_set_hl(0, 'SpikeSemantic', { fg = 0x00ff00 })
  local ns = vim.api.nvim_create_namespace('magit-lsp-spike')
  -- hl_eol is set on purpose: only the explicit 0..4 range should be styled.
  vim.api.nvim_buf_set_extmark(buf, ns, 0, 0, {
    end_col = 4,
    hl_group = 'SpikeRegular',
    hl_eol = true,
  })
  -- Same shape as `vim.lsp.semantic_tokens`'s namespace.
  local sem_ns = vim.api.nvim_create_namespace('nvim.lsp.semantic_tokens')
  vim.api.nvim_buf_set_extmark(buf, sem_ns, 0, 5, {
    end_col = 9,
    hl_group = 'SpikeSemantic',
  })

  local warnings = {}
  -- Warm `tohtml`'s terminal-colour caches so the only notification left is the
  -- semantic-token one; its warnings are batched and only the first is named.
  tohtml.tohtml(win, {})
  vim.wait(300, function()
    return false
  end, 10)

  local orig_notify = vim.notify
  vim.notify = function(msg, ...)
    warnings[#warnings + 1] = tostring(msg)
    return orig_notify(msg, ...)
  end
  local html = tohtml.tohtml(win, {})
  -- `tohtml` emits its warnings from a scheduled callback, so drain the loop.
  vim.wait(200, function()
    return #warnings > 0
  end, 10)
  vim.notify = orig_notify

  vim.api.nvim_buf_delete(buf, { force = true })

  local text = table.concat(html, '\n')
  local function styled(word)
    -- The span carrying `word` must have a non-empty class and a colour rule.
    for line in text:gmatch('[^\n]+') do
      if line:find('<span class="[^"]*">' .. word .. '</span>') then
        return true
      end
    end
    return false
  end
  local semantic_warned = false
  for _, w in ipairs(warnings) do
    if w:find('semantic tokens are not supported') then
      semantic_warned = true
    end
  end

  return {
    regular_extmark = styled('AAAA'),
    semantic_extmark = styled('BBBB'),
    semantic_warned = semantic_warned,
    hl_eol_tail = text:find('AAAA <span') ~= nil,
    warnings = warnings,
  }
end

--- Every distinct token in the buffer, found by probing each position.
---@return table[] { type, modifiers, group, line, start_col, end_col }
local function collect_tokens(buf, lines)
  local seen, tokens = {}, {}
  for row = 0, #lines - 1 do
    local text = lines[row + 1] or ''
    for col = 0, #text do
      for _, token in ipairs(vim.lsp.semantic_tokens.get_at_pos(buf, row, col) or {}) do
        local key = table.concat({
          token.type,
          token.start_col,
          token.end_col,
          token.line,
          vim.inspect(token.modifiers),
        }, '|')
        if not seen[key] then
          seen[key] = true
          tokens[#tokens + 1] = token
        end
      end
    end
  end
  return tokens
end

make_fixture()
vim.fn.mkdir(out_dir, 'p')

local M = require('magit-lsp')
local ok, err = M.setup({ root_dir = repo })
if not ok then
  error('magit-lsp setup failed: ' .. tostring(err))
end
if
  not vim.wait(15000, function()
    return M.status ~= nil
  end, 20)
then
  error('status buffer did not render')
end

-- Highlighting is requested per render; wait until the first tokens arrive.
if
  not vim.wait(15000, function()
    for row = 0, #M.lines - 1 do
      local found = vim.lsp.semantic_tokens.get_at_pos(M.buf, row, 0)
      if found and #found > 0 then
        return true
      end
    end
    return false
  end, 20)
then
  error('semantic tokens did not arrive')
end

local win = vim.api.nvim_get_current_win()
local buf = M.buf
-- Folds are enabled by the client; closed folds would hide content from HTML.
vim.wo[win].foldenable = false

local client = vim.lsp.get_client_by_id(M.client_id)
local legend = client.server_capabilities.semanticTokensProvider.legend

local tokens = collect_tokens(buf, M.lines)
local used_types, used_modifiers = {}, {}
for _, token in ipairs(tokens) do
  used_types[token.type] = true
  for modifier in pairs(token.modifiers) do
    used_modifiers[modifier] = true
  end
end

-- Every group the client is expected to define, in legend order.
local groups = {}
for _, token_type in ipairs(legend.tokenTypes) do
  groups[#groups + 1] = {
    group = '@lsp.type.' .. token_type .. '.magit-lsp',
    token = token_type,
    used = used_types[token_type] == true,
  }
end
for _, modifier in ipairs(legend.tokenModifiers) do
  groups[#groups + 1] = {
    group = '@lsp.mod.' .. modifier .. '.magit-lsp',
    token = modifier,
    used = used_modifiers[modifier] == true,
  }
end

vim.cmd('packadd nvim.tohtml')
local tohtml = require('tohtml')

local cases = {
  { 'default', 'dark' },
  { 'default', 'light' },
  { 'vim', 'dark' },
  { 'habamax', 'dark' },
  { 'delek', 'dark' },
  { 'lunaperche', 'dark' },
}

local results = {}
local background = {}
local links = {}
for _, case in ipairs(cases) do
  local scheme, bg = case[1], case[2]
  vim.o.background = bg
  vim.cmd('colorscheme ' .. scheme)
  -- The client re-applies its links from a `ColorScheme` autocmd; record what
  -- survived instead of re-defining anything here.
  local column = {}
  for _, entry in ipairs(groups) do
    local link = vim.api.nvim_get_hl(0, { name = entry.group, link = true }).link
    links[entry.group] = link
    local resolved = vim.api.nvim_get_hl(0, { name = entry.group, link = false })
    column[entry.group] = hex(resolved.fg) .. (resolved.bold and '*' or '')
  end

  local normal = vim.api.nvim_get_hl(0, { name = 'Normal', link = false })
  background[scheme .. '/' .. bg] = hex(normal.bg)

  local html = tohtml.tohtml(win, { width = 120 })
  local path = string.format('%s/status-%s-%s.html', out_dir, scheme, bg)
  vim.fn.writefile(html, path)
  results[scheme .. '/' .. bg] = { fg = column, html = table.concat(html, '\n'), path = path }
end

-- Report -------------------------------------------------------------------

print(
  ('IMM-8 colour spike — Neovim %s')
    :format(vim.version().major .. '.' .. vim.version().minor .. '.' .. vim.version().patch)
)
print(('fixture: %s'):format(repo))
print(('html:    %s/status-<scheme>-<background>.html'):format(out_dir))
print('')
print('Highlighting comes from semantic tokens: the client only links')
print('`@lsp.type.<token>.<filetype>` and `@lsp.mod.<modifier>.<filetype>` groups.')
print('"-" = no guifg, "*" = bold, "used" = the fixture emits that token.')
print('')

local columns = {}
for _, case in ipairs(cases) do
  columns[#columns + 1] = case[1] .. '/' .. case[2]
end

local function pad(s, n)
  s = tostring(s)
  if #s >= n then
    return s
  end
  return s .. string.rep(' ', n - #s)
end

local head = pad('group', 40) .. pad('link', 14) .. pad('used', 6)
for _, c in ipairs(columns) do
  head = head .. pad(c, 18)
end
print(head)
print(string.rep('-', #head))
local legend_rows = {}
for _, entry in ipairs(groups) do
  local row = pad(entry.group, 40) .. pad(links[entry.group] or 'MISSING', 14)
  row = row .. pad(entry.used and 'yes' or '-', 6)
  for _, c in ipairs(columns) do
    row = row .. pad(results[c].fg[entry.group], 18)
  end
  print(row)
  legend_rows[#legend_rows + 1] = ('| `%s` | %s | %s | %s |'):format(
    entry.group,
    links[entry.group] or 'MISSING',
    entry.used and 'yes' or '',
    results[columns[1]].fg[entry.group]
  )
end
print('')
local bg_row = pad('(Normal bg)', 40) .. pad('', 14) .. pad('', 6)
for _, c in ipairs(columns) do
  bg_row = bg_row .. pad(background[c], 18)
end
print(bg_row)
print('')

-- Is each group's class present in the HTML? `tohtml` names classes after the
-- resolved group, so check the link target rather than the token name.
print('class present in HTML (semantic-token extmarks are skipped by tohtml):')
local first = results[columns[1]].html
for _, entry in ipairs(groups) do
  local name = links[entry.group] or resolved_name(entry.group)
  local found = first:find('class="' .. name .. '"', 1, true) ~= nil
  print(('  %s%s -> %s'):format(pad(entry.group, 40), found and 'yes' or 'NO ', name))
end
print('')

local missing = {}
local uncoloured = {}
for _, entry in ipairs(groups) do
  if not links[entry.group] then
    missing[#missing + 1] = entry.group
  end
  local coloured = false
  for _, c in ipairs(columns) do
    if results[c].fg[entry.group] ~= '-' then
      coloured = true
    end
  end
  if not coloured then
    uncoloured[#uncoloured + 1] = entry.group
  end
end
if #missing > 0 then
  print('groups without a theme link:')
  for _, group in ipairs(missing) do
    print('  ' .. group)
  end
  print('')
end
if #uncoloured > 0 then
  print('no foreground in any tested scheme (attribute-only groups are expected):')
  for _, group in ipairs(uncoloured) do
    print('  ' .. group)
  end
  print('')
end

local legend_path = out_dir .. '/semantic-tokens.md'
local legend_lines = {
  '# Semantic token legend (generated by `make color-spike`)',
  '',
  '| group | link | used in fixture | fg (default/dark) |',
  '| --- | --- | --- | --- |',
}
for _, row in ipairs(legend_rows) do
  legend_lines[#legend_lines + 1] = row
end
vim.fn.writefile(legend_lines, legend_path)

print(('wrote %d HTML files and %s'):format(#cases, legend_path))
print('')

local probe = probe_tohtml()
print('tohtml probes:')
print(('  regular extmark highlight included:   %s'):format(tostring(probe.regular_extmark)))
print(('  semantic-token extmark included:      %s'):format(tostring(probe.semantic_extmark)))
print(('  semantic-token warning emitted:       %s'):format(tostring(probe.semantic_warned)))
print(('  hl_eol tail styled beyond end_col:    %s'):format(tostring(probe.hl_eol_tail)))
for _, w in ipairs(probe.warnings) do
  print(('  warning: %s'):format(w))
end
print('')
print('done')
