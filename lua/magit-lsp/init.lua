local M = {}

M.map = {}
M.lines = {}
M.ns = vim.api.nvim_create_namespace('magit-lsp')

local function project_root()
  local source = debug.getinfo(1, 'S').source
  local path = source:sub(2)
  path = vim.fn.fnamemodify(path, ':h') -- lua/magit-lsp
  path = vim.fn.fnamemodify(path, ':h') -- lua
  return vim.fn.fnamemodify(path, ':h') -- project root
end

local function default_cmd()
  local bin = project_root() .. '/target/debug/magit-lsp'
  if vim.fn.filereadable(bin) == 1 then
    return { bin }
  end
  return { 'cargo', 'run', '--manifest-path', project_root() .. '/Cargo.toml' }
end

local function is_nil(v)
  return v == nil or v == vim.NIL
end

local LEVELS = {
  info = vim.log.levels.INFO,
  warn = vim.log.levels.WARN,
  error = vim.log.levels.ERROR,
}

local function error_message(err)
  if type(err) == 'table' then
    return tostring(err.message or err)
  end
  return tostring(err)
end

--- Show one notification per message effect, at the requested level.
local function apply_effects(effects)
  for _, effect in ipairs(effects or {}) do
    if effect.type == 'message' then
      M.last_error_detail = effect.detail
      local text = effect.text or ''
      if effect.hint then
        text = text .. '\n' .. effect.hint
      end
      vim.notify(text, LEVELS[effect.level] or vim.log.levels.INFO)
    end
  end
end

local function define_highlights()
  local hl = {
    { 'MagitHeader', 'Title' },
    { 'MagitSection', 'Function' },
    { 'MagitFileModified', 'DiagnosticWarn' },
    { 'MagitFileAdded', 'DiagnosticOk' },
    { 'MagitFileDeleted', 'DiagnosticError' },
    { 'MagitFileUntracked', 'Comment' },
    { 'MagitHunkHeader', 'Special' },
    { 'MagitDiffAdded', 'diffAdded' },
    { 'MagitDiffRemoved', 'diffRemoved' },
    { 'MagitMessage', 'Comment' },
  }
  for _, g in ipairs(hl) do
    vim.api.nvim_set_hl(0, g[1], { link = g[2], default = true })
  end
end

local function wait_ready(timeout)
  local client = M.client_id and vim.lsp.get_client_by_id(M.client_id)
  if not client or client.initialized then
    return true
  end
  return vim.wait(timeout or 5000, function()
    return client.initialized
  end, 10)
end

local function request_sync(method, params)
  wait_ready()
  for _ = 1, 20 do
    local resp = vim.lsp.buf_request_sync(M.buf, method, params, 5000)
    if resp then
      for _, r in pairs(resp) do
        if r.result ~= nil then
          return r.result, nil
        end
        if r.err then
          return nil, r.err
        end
      end
      return nil, 'empty response for ' .. method
    end
    vim.wait(100)
  end
  return nil, 'timed out waiting for ' .. method
end

local function file_hl(status)
  if status == 'A' then
    return 'MagitFileAdded'
  elseif status == 'D' then
    return 'MagitFileDeleted'
  elseif status == '?' then
    return 'MagitFileUntracked'
  end
  return 'MagitFileModified'
end

local function highlight(lines, map)
  vim.api.nvim_buf_clear_namespace(M.buf, M.ns, 0, -1)
  for i, e in ipairs(map) do
    local hl
    if e.kind == 'header' then
      hl = 'MagitHeader'
    elseif e.kind == 'section' then
      hl = 'MagitSection'
    elseif e.kind == 'message' then
      hl = 'MagitMessage'
    elseif e.kind == 'hunk' then
      hl = 'MagitHunkHeader'
    elseif e.kind == 'file' then
      hl = file_hl(e.status)
    elseif e.kind == 'line' then
      if e.marker == '+' then
        hl = 'MagitDiffAdded'
      elseif e.marker == '-' then
        hl = 'MagitDiffRemoved'
      end
    end
    if hl then
      local text = lines[i] or ''
      vim.api.nvim_buf_set_extmark(M.buf, M.ns, i - 1, 0, {
        end_row = i - 1,
        end_col = math.max(1, #text),
        hl_group = hl,
        hl_eol = true,
      })
    end
  end
end

local function render(status)
  local lines = {}
  local map = {}
  local function add(text, entry)
    table.insert(lines, text)
    table.insert(map, entry or { kind = 'text' })
  end

  local branch = status.branch
  if is_nil(branch) or branch == '' then
    branch = 'HEAD'
  end
  add('Head: ' .. branch, { kind = 'header' })

  local function render_files(title, files, section)
    add('', { kind = 'blank' })
    add(string.format('%s (%d)', title, #files), { kind = 'section', section = section })
    for _, f in ipairs(files) do
      add('  ' .. f.status .. ' ' .. f.path, {
        kind = 'file',
        section = section,
        path = f.path,
        status = f.status,
      })
      for hi, h in ipairs(f.hunks) do
        add(
          string.format('    @@ -%d,%d +%d,%d @@', h.old_start, h.old_count, h.new_start, h.new_count),
          { kind = 'hunk', section = section, path = f.path, hunk_index = hi - 1 }
        )
        for li, l in ipairs(h.lines) do
          add('    ' .. l.marker .. l.text, {
            kind = 'line',
            section = section,
            path = f.path,
            hunk_index = hi - 1,
            line_index = li - 1,
            marker = l.marker,
            text = l.text,
          })
        end
      end
    end
  end

  local modified, untracked = {}, {}
  for _, f in ipairs(status.unstaged or {}) do
    if f.status == '?' then
      table.insert(untracked, f)
    else
      table.insert(modified, f)
    end
  end

  render_files('Staged changes', status.staged or {}, 'staged')
  render_files('Unstaged changes', modified, 'unstaged')
  render_files('Untracked files', untracked, 'untracked')

  if #(status.staged or {}) == 0 and #modified == 0 and #untracked == 0 then
    add('', { kind = 'blank' })
    add('Working tree clean', { kind = 'message' })
  end

  vim.api.nvim_buf_set_lines(M.buf, 0, -1, false, lines)
  M.lines = lines
  M.map = map
  M.status = status

  highlight(lines, map)

  if M.client_id then
    vim.lsp.buf_notify(M.buf, 'vcs/didRender', {
      uri = vim.uri_from_bufnr(M.buf),
      text = table.concat(lines, '\n'),
    })
  end
end

--- Fetch the current status and re-render the buffer.
---@return table|nil status
function M.refresh()
  local status, err = request_sync('vcs/status', { repo_path = M.root })
  if not status then
    M.last_error_detail = type(err) == 'table' and err.data and err.data.detail or nil
    vim.notify('magit-lsp refresh failed: ' .. error_message(err), vim.log.levels.WARN)
    return nil, err
  end
  render(status)
  return status
end

local function apply_mutation(method, params)
  local resp, err = request_sync(method, params)
  if not resp then
    M.last_error_detail = type(err) == 'table' and err.data and err.data.detail or nil
    vim.notify('magit-lsp ' .. method .. ' failed: ' .. error_message(err), vim.log.levels.WARN)
    return nil, err
  end
  render(resp.status)
  apply_effects(resp.effects)
  return resp.status
end

--- Stage a whole hunk of a file.
function M.stage_hunk(path, hunk_index)
  return apply_mutation('vcs/stageHunk', { repo_path = M.root, path = path, hunk_index = hunk_index })
end

--- Unstage a whole hunk of a file.
function M.unstage_hunk(path, hunk_index)
  return apply_mutation('vcs/unstageHunk', { repo_path = M.root, path = path, hunk_index = hunk_index })
end

--- Stage lines `start..finish` (0-based, within the hunk) of a file.
function M.stage_lines(path, hunk_index, start, finish)
  return apply_mutation('vcs/stageLines', {
    repo_path = M.root,
    path = path,
    hunk_index = hunk_index,
    start = start,
    ['end'] = finish,
  })
end

--- Unstage lines `start..finish` (0-based, within the hunk) of a file.
function M.unstage_lines(path, hunk_index, start, finish)
  return apply_mutation('vcs/unstageLines', {
    repo_path = M.root,
    path = path,
    hunk_index = hunk_index,
    start = start,
    ['end'] = finish,
  })
end

--- Stage an entire file (including untracked files).
function M.stage_file(path)
  return apply_mutation('vcs/stageFile', { repo_path = M.root, path = path })
end

--- Unstage an entire file.
function M.unstage_file(path)
  return apply_mutation('vcs/unstageFile', { repo_path = M.root, path = path })
end

--- Stage all tracked changes.
function M.stage_all()
  return apply_mutation('vcs/stageAll', { repo_path = M.root })
end

--- Unstage everything.
function M.unstage_all()
  return apply_mutation('vcs/unstageAll', { repo_path = M.root })
end

--- Fetch the unified diff for a file (staged or unstaged).
function M.diff(path, staged)
  local resp, err = request_sync('vcs/diff', {
    repo_path = M.root,
    path = path,
    staged = staged or false,
  })
  if not resp then
    M.last_error_detail = type(err) == 'table' and err.data and err.data.detail or nil
    vim.notify('magit-lsp diff failed: ' .. error_message(err), vim.log.levels.WARN)
    return nil, err
  end
  return resp
end

--- Fetch navigation targets (section/file/hunk) from the server.
function M.targets()
  local resp, err = request_sync('vcs/targets', { uri = vim.uri_from_bufnr(M.buf) })
  if not resp then
    return nil, err
  end
  return resp.targets
end

--- Move the cursor to the next/previous target of `kind` (`dir` is 1 or -1).
function M.jump(kind, dir)
  local targets, err = M.targets()
  if not targets then
    return nil, err
  end
  local cur = vim.fn.line('.')
  local lines = {}
  for _, t in ipairs(targets) do
    if kind == 'any' or t.kind == kind then
      table.insert(lines, t.line)
    end
  end
  table.sort(lines)
  if dir > 0 then
    for _, l in ipairs(lines) do
      if l > cur then
        vim.api.nvim_win_set_cursor(0, { l, 0 })
        return l
      end
    end
  else
    for i = #lines, 1, -1 do
      if lines[i] < cur then
        vim.api.nvim_win_set_cursor(0, { lines[i], 0 })
        return lines[i]
      end
    end
  end
  return nil, 'no ' .. kind .. ' target'
end

local function cursor_entry()
  return M.map[vim.fn.line('.')]
end

--- Stage the file or hunk under the cursor (lines only via visual mode).
function M.stage_at()
  local e = cursor_entry()
  if not e then
    return nil, 'nothing under cursor'
  end
  if e.section ~= 'unstaged' and e.section ~= 'untracked' then
    return nil, 'no unstaged item under cursor'
  end
  if e.kind == 'file' then
    return M.stage_file(e.path)
  end
  if e.kind == 'hunk' or e.kind == 'line' then
    return M.stage_hunk(e.path, e.hunk_index)
  end
  return nil, 'nothing to stage here'
end

--- Unstage the file or hunk under the cursor.
function M.unstage_at()
  local e = cursor_entry()
  if not e or e.section ~= 'staged' then
    return nil, 'no staged item under cursor'
  end
  if e.kind == 'file' then
    return M.unstage_file(e.path)
  end
  if e.kind == 'hunk' or e.kind == 'line' then
    return M.unstage_hunk(e.path, e.hunk_index)
  end
  return nil, 'nothing to unstage here'
end

--- Stage the lines between buffer lines `from` and `to` (inclusive).
function M.stage_line_range(from, to)
  local a = M.map[from]
  local b = M.map[to]
  if not a or not b or a.kind ~= 'line' or b.kind ~= 'line' then
    return nil, 'selection must be on change lines'
  end
  if a.path ~= b.path or a.hunk_index ~= b.hunk_index then
    return nil, 'selection spans multiple hunks'
  end
  if a.section ~= 'unstaged' then
    return nil, 'not in unstaged section'
  end
  local lo = math.min(a.line_index, b.line_index)
  local hi = math.max(a.line_index, b.line_index)
  return M.stage_lines(a.path, a.hunk_index, lo, hi)
end

--- Unstage the lines between buffer lines `from` and `to` (inclusive).
function M.unstage_line_range(from, to)
  local a = M.map[from]
  local b = M.map[to]
  if not a or not b or a.kind ~= 'line' or b.kind ~= 'line' then
    return nil, 'selection must be on change lines'
  end
  if a.section ~= 'staged' then
    return nil, 'not in staged section'
  end
  local lo = math.min(a.line_index, b.line_index)
  local hi = math.max(a.line_index, b.line_index)
  return M.unstage_lines(a.path, a.hunk_index, lo, hi)
end

--- Stage the current visual selection (visual-mode mapping target).
function M.stage_visual()
  local a, b = vim.fn.line('v'), vim.fn.line('.')
  return M.stage_line_range(math.min(a, b), math.max(a, b))
end

--- Unstage the current visual selection (visual-mode mapping target).
function M.unstage_visual()
  local a, b = vim.fn.line('v'), vim.fn.line('.')
  return M.unstage_line_range(math.min(a, b), math.max(a, b))
end

--- Open a file's diff in a side split.
function M.show_diff(path, staged)
  local resp, err = M.diff(path, staged)
  if not resp then
    return nil, err
  end
  local dbuf = vim.api.nvim_create_buf(false, true)
  vim.bo[dbuf].buftype = 'nofile'
  vim.bo[dbuf].bufhidden = 'hide'
  vim.bo[dbuf].swapfile = false
  vim.api.nvim_buf_set_name(dbuf, 'magit-lsp://diff/' .. path)
  local dlines = vim.split(resp.diff, '\n', { plain = true })
  vim.api.nvim_buf_set_lines(dbuf, 0, -1, false, dlines)
  vim.cmd('botright vsplit')
  vim.api.nvim_win_set_buf(0, dbuf)
  M.diff_buf = dbuf
  return resp
end

--- Open the diff for the file under the cursor.
function M.diff_at()
  local e = cursor_entry()
  if not e or not e.path then
    return nil, 'no file under cursor'
  end
  return M.show_diff(e.path, e.section == 'staged')
end

--- Open the detail of the last error in a scratch split.
function M.error_detail()
  local detail = M.last_error_detail
  if not detail or detail == '' then
    vim.notify('magit-lsp: no error detail to show', vim.log.levels.INFO)
    return
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].buftype = 'nofile'
  vim.bo[buf].bufhidden = 'hide'
  vim.bo[buf].swapfile = false
  vim.api.nvim_buf_set_name(buf, 'magit-lsp://error')
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(detail, '\n', { plain = true }))
  vim.cmd('botright split')
  vim.api.nvim_win_set_buf(0, buf)
end

local function keymaps()
  local opts = { buffer = M.buf, nowait = true, noremap = true, silent = true }
  vim.keymap.set('n', 's', M.stage_at, vim.tbl_extend('force', opts, { desc = 'Stage file/hunk' }))
  vim.keymap.set('n', 'u', M.unstage_at, vim.tbl_extend('force', opts, { desc = 'Unstage file/hunk' }))
  vim.keymap.set('x', 's', M.stage_visual, vim.tbl_extend('force', opts, { desc = 'Stage selected lines' }))
  vim.keymap.set('x', 'u', M.unstage_visual, vim.tbl_extend('force', opts, { desc = 'Unstage selected lines' }))
  vim.keymap.set('n', 'S', M.stage_all, vim.tbl_extend('force', opts, { desc = 'Stage all' }))
  vim.keymap.set('n', 'U', M.unstage_all, vim.tbl_extend('force', opts, { desc = 'Unstage all' }))
  vim.keymap.set('n', '<CR>', M.diff_at, vim.tbl_extend('force', opts, { desc = 'Show diff' }))
  vim.keymap.set('n', 'gR', M.refresh, vim.tbl_extend('force', opts, { desc = 'Refresh status' }))
  vim.keymap.set('n', ']s', function()
    M.jump('section', 1)
  end, vim.tbl_extend('force', opts, { desc = 'Next section' }))
  vim.keymap.set('n', '[s', function()
    M.jump('section', -1)
  end, vim.tbl_extend('force', opts, { desc = 'Previous section' }))
  vim.keymap.set('n', ']f', function()
    M.jump('file', 1)
  end, vim.tbl_extend('force', opts, { desc = 'Next file' }))
  vim.keymap.set('n', '[f', function()
    M.jump('file', -1)
  end, vim.tbl_extend('force', opts, { desc = 'Previous file' }))
  vim.keymap.set('n', ']h', function()
    M.jump('hunk', 1)
  end, vim.tbl_extend('force', opts, { desc = 'Next hunk' }))
  vim.keymap.set('n', '[h', function()
    M.jump('hunk', -1)
  end, vim.tbl_extend('force', opts, { desc = 'Previous hunk' }))
end

local function enable_folding(win)
  vim.wo[win].foldmethod = 'expr'
  vim.wo[win].foldexpr = 'v:lua.vim.lsp.foldexpr()'
  vim.wo[win].foldenable = true
  vim.wo[win].foldlevel = 99
end

local function fresh_buffer()
  local existing = vim.fn.bufnr('vcs://status')
  if existing ~= -1 then
    vim.api.nvim_buf_delete(existing, { force = true })
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].buftype = 'nofile'
  vim.bo[buf].bufhidden = 'hide'
  vim.bo[buf].swapfile = false
  vim.api.nvim_buf_set_name(buf, 'vcs://status')
  return buf
end

--- Start the server and open the status buffer.
---@param opts table|nil { root_dir?: string, cmd?: string[] }
---@return table|nil status
function M.setup(opts)
  opts = opts or {}
  M.root = opts.root_dir or vim.fn.getcwd()
  local cmd = opts.cmd or default_cmd()

  define_highlights()

  vim.api.nvim_create_user_command('MagitLspErrorDetail', M.error_detail, {
    desc = 'Show the last magit-lsp error detail',
    force = true,
  })

  M.buf = fresh_buffer()

  M.client_id = vim.lsp.start({
    name = 'magit-lsp',
    cmd = cmd,
    root_dir = M.root,
  }, { bufnr = M.buf })

  if not M.client_id then
    vim.notify('Failed to start magit-lsp server', vim.log.levels.ERROR)
    return nil, 'failed to start client'
  end

  keymaps()

  vim.api.nvim_create_autocmd('VimLeavePre', {
    callback = function()
      local client = M.client_id and vim.lsp.get_client_by_id(M.client_id)
      if client then
        client:stop(true)
      end
    end,
  })

  vim.cmd('vsplit')
  local win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, M.buf)
  enable_folding(win)

  return M.refresh()
end

return M