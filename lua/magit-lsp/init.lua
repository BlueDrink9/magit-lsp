local M = {}

M.map = {}
M.lines = {}

--- The status buffer's filetype; LSP groups are `@lsp.type.<type>.<filetype>`.
M.filetype = 'magit-lsp'

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

--- Standard theme groups for every token type of TASKS.md §3.15.
--- The server names the token types; the client only links them to the
--- colourscheme's own groups, so no colour is ever hard-coded here.
local TOKEN_LINKS = {
  ['magit.section'] = 'Title',
  ['magit.count'] = 'Comment',
  ['magit.file'] = 'Directory',
  ['magit.file.modified'] = 'Changed',
  ['magit.file.added'] = 'Added',
  ['magit.file.deleted'] = 'Removed',
  ['magit.file.renamed'] = 'Changed',
  ['magit.file.untracked'] = 'Comment',
  ['magit.hunk.header'] = 'Special',
  ['magit.diff.added'] = '@diff.plus',
  ['magit.diff.removed'] = '@diff.minus',
  ['magit.diff.context'] = 'Normal',
  ['magit.hash'] = 'Constant',
  ['magit.ref.local'] = 'Type',
  ['magit.ref.remote'] = 'Special',
  ['magit.ref.tag'] = 'Label',
  ['magit.ref.head'] = 'Type',
  ['magit.author'] = 'Identifier',
  ['magit.date'] = 'Comment',
  ['magit.message.warn'] = 'WarningMsg',
  ['magit.message.error'] = 'ErrorMsg',
  ['magit.menu.title'] = 'Title',
  ['magit.menu.group'] = 'Statement',
  ['magit.menu.key'] = 'Keyword',
  ['magit.menu.label'] = 'Normal',
  ['magit.menu.flag'] = 'Special',
  ['magit.menu.switch.on'] = 'String',
  ['magit.menu.switch.off'] = 'Comment',
  ['magit.menu.option'] = 'String',
  ['magit.menu.value'] = 'String',
  ['magit.menu.planned'] = 'Comment',
  ['magit.menu.disabled'] = 'Comment',
  ['magit.menu.preview'] = 'Comment',
}

--- Modifier groups are applied above the type, so they should only add
--- attributes (or a background) and never replace a type's foreground.
local MODIFIER_LINKS = {
  staged = 'Bold',
  selected = 'Visual',
  planned = 'Comment',
  disabled = 'Comment',
  bold = 'Bold',
}

--- Link every semantic-token group to a standard theme group.
--- `default = true` keeps a user's own definition; `:colorscheme` clears the
--- groups, so this runs again on `ColorScheme`.
local function define_highlights()
  for token_type, link in pairs(TOKEN_LINKS) do
    local group = string.format('@lsp.type.%s.%s', token_type, M.filetype)
    vim.api.nvim_set_hl(0, group, { link = link, default = true })
  end
  for modifier, link in pairs(MODIFIER_LINKS) do
    local group = string.format('@lsp.mod.%s.%s', modifier, M.filetype)
    vim.api.nvim_set_hl(0, group, { link = link, default = true })
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

--- Ask Neovim to re-request semantic tokens for the buffer.
--- The server owns highlighting, but it only sends tokens on request, so a
--- re-render has to trigger one.
local function refresh_tokens()
  vim.lsp.semantic_tokens.force_refresh(M.buf)
end

--- Replace every line in the status buffer.
--- The view is read-only, so this is the only place allowed to write to it.
---@param buf integer
---@param lines string[]
local function set_lines(buf, lines)
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modified = false
  vim.bo[buf].modifiable = false
end

--- Show a server-rendered document in the status buffer.
--- The server owns the text and the per-line node map; the client only
--- replaces lines and asks for the matching semantic tokens.
---@param status table structured status (kept for callers and tests)
---@param document table|nil { text = string[], nodes = table[], version = integer }
local function render(status, document)
  if type(document) ~= 'table' or type(document.text) ~= 'table' then
    return nil, 'server response has no document'
  end
  local lines = document.text
  set_lines(M.buf, lines)
  M.lines = lines
  M.map = document.nodes or {}
  M.status = status
  M.version = document.version

  refresh_tokens()
  return status
end

--- Fetch the current status and replace the buffer with the server's view.
---@return table|nil status
function M.refresh()
  local status, err = request_sync('vcs/status', {
    repo_path = M.root,
    uri = vim.uri_from_bufnr(M.buf),
  })
  if not status then
    M.last_error_detail = type(err) == 'table' and err.data and err.data.detail or nil
    vim.notify('magit-lsp refresh failed: ' .. error_message(err), vim.log.levels.WARN)
    return nil, err
  end
  render(status, status.document)
  return status
end

local function apply_mutation(method, params)
  local resp, err = request_sync(method, params)
  if not resp then
    M.last_error_detail = type(err) == 'table' and err.data and err.data.detail or nil
    vim.notify('magit-lsp ' .. method .. ' failed: ' .. error_message(err), vim.log.levels.WARN)
    return nil, err
  end
  render(resp.status, resp.document)
  apply_effects(resp.effects)
  return resp.status
end

--- Stage a whole hunk of a file.
function M.stage_hunk(path, hunk_id)
  return apply_mutation('vcs/stageHunk', { repo_path = M.root, path = path, hunk_id = hunk_id })
end

--- Unstage a whole hunk of a file.
function M.unstage_hunk(path, hunk_id)
  return apply_mutation('vcs/unstageHunk', { repo_path = M.root, path = path, hunk_id = hunk_id })
end

--- Stage lines `start..finish` (0-based, within the hunk) of a file.
function M.stage_lines(path, hunk_id, start, finish)
  return apply_mutation('vcs/stageLines', {
    repo_path = M.root,
    path = path,
    hunk_id = hunk_id,
    start = start,
    ['end'] = finish,
  })
end

--- Unstage lines `start..finish` (0-based, within the hunk) of a file.
function M.unstage_lines(path, hunk_id, start, finish)
  return apply_mutation('vcs/unstageLines', {
    repo_path = M.root,
    path = path,
    hunk_id = hunk_id,
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

--- Stage all untracked files, leaving tracked changes unstaged.
function M.stage_untracked()
  return apply_mutation('vcs/stageUntracked', { repo_path = M.root })
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
  if e.kind == 'section' and e.section == 'untracked' then
    return M.stage_untracked()
  end
  if e.kind == 'file' then
    return M.stage_file(e.path)
  end
  if e.kind == 'hunk' or e.kind == 'line' then
    return M.stage_hunk(e.path, e.hunk_id)
  end
  return nil, 'nothing to stage here'
end

--- Unstage the file or hunk under the cursor.
function M.unstage_at()
  local e = cursor_entry()
  if e and e.section == 'untracked' then
    vim.notify('Nothing to unstage here', vim.log.levels.INFO)
    return nil, 'nothing to unstage here'
  end
  if not e or e.section ~= 'staged' then
    return nil, 'no staged item under cursor'
  end
  if e.kind == 'file' then
    return M.unstage_file(e.path)
  end
  if e.kind == 'hunk' or e.kind == 'line' then
    return M.unstage_hunk(e.path, e.hunk_id)
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
  if a.path ~= b.path or a.hunk_id ~= b.hunk_id then
    return nil, 'selection spans multiple hunks'
  end
  if a.section ~= 'unstaged' then
    return nil, 'not in unstaged section'
  end
  local lo = math.min(a.line_index, b.line_index)
  local hi = math.max(a.line_index, b.line_index)
  return M.stage_lines(a.path, a.hunk_id, lo, hi)
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
  return M.unstage_lines(a.path, a.hunk_id, lo, hi)
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

-- Text mode will replace this wording once it exists.
local READ_ONLY_HINT = 'Read-only view. Text mode (to edit/copy as text) is not available yet.'

local WRITE_COMMANDS = { w = true, wr = true, wri = true, writ = true, write = true }

--- True when `cmdline` is a bare `:w`/`:write` (any abbreviation, optional `!`).
local function is_write_command(cmdline)
  local cmd = cmdline:match('^%s*(%a+)!?%s*$')
  return cmd ~= nil and WRITE_COMMANDS[cmd] == true
end

--- Show the read-only hint once per session.
local function notify_read_only()
  if M.read_only_hint_shown then
    return
  end
  M.read_only_hint_shown = true
  vim.notify(READ_ONLY_HINT, vim.log.levels.INFO)
end

--- Make `buf` a read-only view of server-owned text.
---@param buf integer
local function configure_read_only(buf)
  vim.api.nvim_create_autocmd('BufWriteCmd', {
    buffer = buf,
    callback = function()
      vim.bo[buf].modified = false
      notify_read_only()
    end,
  })

  -- `nofile` buffers reject `:w` with E382 before BufWriteCmd runs, so also
  -- swallow a bare write at the command line. Clearing the command line
  -- cancels it; pcall because that only works while cmdline mode is active.
  --
  -- Native E21 edit failures have no autocmd to observe, so the hint is shown
  -- on the first attempted write for now; text mode will hook real detection.
  vim.api.nvim_create_autocmd('CmdlineLeave', {
    buffer = buf,
    callback = function()
      if vim.fn.getcmdtype() ~= ':' or not is_write_command(vim.fn.getcmdline()) then
        return
      end
      pcall(vim.fn.setcmdline, '')
      vim.bo[buf].modified = false
      notify_read_only()
    end,
  })
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
  vim.bo[buf].modifiable = false
  vim.bo[buf].readonly = true
  -- The view is server-owned and refresh replaces it wholesale.
  vim.bo[buf].undolevels = -1
  vim.bo[buf].modified = false
  vim.api.nvim_buf_set_name(buf, 'vcs://status')
  -- A dedicated filetype keeps the `@lsp.type.*.<filetype>` group names stable.
  vim.bo[buf].filetype = M.filetype
  configure_read_only(buf)
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

  -- `:colorscheme` runs `hi clear`, so re-apply the links afterwards.
  vim.api.nvim_create_autocmd('ColorScheme', {
    group = vim.api.nvim_create_augroup('magit_lsp_highlights', { clear = true }),
    callback = define_highlights,
  })

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