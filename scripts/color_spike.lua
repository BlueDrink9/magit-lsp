-- IMM-8 spike: capture the rendered status buffer as HTML with `:TOhtml` and
-- report the resolved colours of every `Magit*` highlight group.
--
-- Run (from the project root):
--   nvim --headless --noplugin -u scripts/minimal_init.lua -l scripts/color_spike.lua
-- or:
--   make color-spike
--
-- Prototype only: touches no production code. Writes
-- `build/test-output/status-<scheme>-<background>.html` and prints a table of
-- highlight group -> link target -> resolved foreground per colourscheme.
-- Also probes two `tohtml` questions: are client extmark highlights included,
-- and are LSP semantic-token extmarks included?

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

make_fixture()
vim.fn.mkdir(out_dir, 'p')

local M = require('magit-lsp')
local ok, err = M.setup({ root_dir = repo })
if not ok then
  error('magit-lsp setup failed: ' .. tostring(err))
end
if not vim.wait(15000, function()
  return M.status ~= nil
end, 20) then
  error('status buffer did not render')
end

local win = vim.api.nvim_get_current_win()
local buf = M.buf
-- Folds are enabled by the client; closed folds would hide content from HTML.
vim.wo[win].foldenable = false

-- Collect the groups actually used by the buffer's extmarks, in a stable order.
local seen, groups = {}, {}
for _, m in ipairs(vim.api.nvim_buf_get_extmarks(buf, M.ns, 0, -1, { details = true })) do
  local g = m[4].hl_group
  if g and not seen[g] then
    seen[g] = true
    groups[#groups + 1] = g
  end
end
table.sort(groups)

-- The colourscheme command clears user groups, so re-apply the links captured
-- from the client instead of duplicating its mapping table here.
local links = {}
for _, g in ipairs(groups) do
  links[g] = vim.api.nvim_get_hl(0, { name = g, link = true }).link
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
for _, case in ipairs(cases) do
  local scheme, bg = case[1], case[2]
  vim.o.background = bg
  vim.cmd('colorscheme ' .. scheme)
  for g, link in pairs(links) do
    if link then
      vim.api.nvim_set_hl(0, g, { link = link, default = true })
    end
  end

  local normal = vim.api.nvim_get_hl(0, { name = 'Normal', link = false })
  background[scheme .. '/' .. bg] = hex(normal.bg)

  local fg = {}
  for _, g in ipairs(groups) do
    local d = vim.api.nvim_get_hl(0, { name = g, link = false })
    fg[g] = hex(d.fg) .. (d.bold and '*' or '')
  end

  local html = tohtml.tohtml(win, { width = 120 })
  local path = string.format('%s/status-%s-%s.html', out_dir, scheme, bg)
  vim.fn.writefile(html, path)
  results[scheme .. '/' .. bg] = { fg = fg, html = table.concat(html, '\n'), path = path }
end

-- Report -------------------------------------------------------------------

print(('IMM-8 colour spike — Neovim %s'):format(vim.version().major .. '.' .. vim.version().minor .. '.' .. vim.version().patch))
print(('fixture: %s'):format(repo))
print(('html:    %s/status-<scheme>-<background>.html'):format(out_dir))
print('')
print('API: require("tohtml").tohtml(winid, opt) -> string[]  (packadd nvim.tohtml first)')
print('     opt = { title?, number_lines?, font?, width?, range? }')
print('     "-" = no guifg, "*" = bold')
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

local head = pad('class', 24) .. pad('link', 18)
for _, c in ipairs(columns) do
  head = head .. pad(c, 18)
end
print(head)
print(string.rep('-', #head))
for _, g in ipairs(groups) do
  local row = pad(g, 24) .. pad(links[g] or '-', 18)
  for _, c in ipairs(columns) do
    row = row .. pad(results[c].fg[g], 18)
  end
  print(row)
end
print('')
local bg_row = pad('(Normal bg)', 24) .. pad('', 18)
for _, c in ipairs(columns) do
  bg_row = bg_row .. pad(background[c], 18)
end
print(bg_row)
print('')

-- Is each group's class present in the HTML? `tohtml` names classes after the
-- resolved group, so check the link target rather than the Magit* name.
print('class present in HTML:')
local first = results[columns[1]].html
for _, g in ipairs(groups) do
  local name = links[g] or resolved_name(g)
  local found = first:find('class="' .. name .. '"', 1, true) ~= nil
  print(('  %s%s -> %s'):format(pad(g, 24), found and 'yes' or 'NO ', name))
end
print('')

local undefined = {}
for _, g in ipairs(groups) do
  local coloured = false
  for _, c in ipairs(columns) do
    if results[c].fg[g] ~= '-' then
      coloured = true
    end
  end
  if not coloured then
    undefined[#undefined + 1] = ('%s -> %s'):format(g, links[g] or '?')
  end
end
if #undefined > 0 then
  print('no foreground in any tested scheme:')
  for _, u in ipairs(undefined) do
    print('  ' .. u)
  end
  print('')
end

print(('wrote %d HTML files to %s'):format(#cases, out_dir))
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
