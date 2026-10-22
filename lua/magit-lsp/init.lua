local M = {}

--- The last text the server sent, for clients and tests that need it.
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

--- The document URI of the view; positional requests must use it verbatim.
local function document_uri()
  return M.uri or vim.uri_from_bufnr(M.buf)
end

--- The only writer of the status buffer: replace `start`..`finish` lines.
--- The view is read-only, so every update goes through here.
---@param buf integer
---@param start integer 0-based first line
---@param finish integer 0-based end line, exclusive; -1 means the last line
---@param lines string[]
local function set_lines(buf, start, finish, lines)
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, start, finish, false, lines)
  vim.bo[buf].modified = false
  vim.bo[buf].modifiable = false
end

--- Apply line-range edits bottom-up so earlier offsets stay valid.
---@param edits table[]
local function apply_edits(edits)
  table.sort(edits, function(a, b)
    return a.startLine > b.startLine
  end)
  for _, edit in ipairs(edits) do
    set_lines(M.buf, edit.startLine, edit.endLine, edit.lines)
  end
end

local function status_window()
  local win = vim.fn.bufwinid(M.buf)
  if win == -1 then
    return nil
  end
  return win
end

--- Move the cursor to the server's successor position (0-based).
---@param cursor table { line = integer, column = integer }
local function set_cursor(cursor)
  local win = status_window()
  if not win then
    return
  end
  local line = math.min(cursor.line, vim.api.nvim_buf_line_count(M.buf) - 1)
  local text = vim.api.nvim_buf_get_lines(M.buf, line, line + 1, false)[1] or ''
  local column = math.min(cursor.column, #text)
  vim.api.nvim_win_set_cursor(win, { line + 1, column })
end

local function ack(version)
  local client = vim.lsp.get_client_by_id(M.client_id)
  if client and M.view_id then
    client:notify('view/rendered', { viewId = M.view_id, version = version })
  end
end

local function request_resync()
  local client = vim.lsp.get_client_by_id(M.client_id)
  if not client or not M.view_id then
    return
  end
  client:request('view/resync', { viewId = M.view_id }, function(err, result)
    if err or type(result) ~= 'table' then
      return
    end
    M.apply_render(result)
  end, M.buf)
end

--- Apply a `view/render` payload, then acknowledge the version.
---
--- A client that does not hold the edit's base version has lost an update and
--- asks for a full resync instead of applying edits to stale text.
---@param params table
function M.apply_render(params)
  if not params or params.viewId ~= M.view_id then
    return
  end

  if params.text then
    set_lines(M.buf, 0, -1, params.text)
  elseif params.edits then
    if params.baseVersion ~= M.version then
      request_resync()
      return
    end
    apply_edits(params.edits)
  end

  M.lines = vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)
  M.version = params.version
  if params.cursor then
    set_cursor(params.cursor)
  end
  -- Ack before asking for tokens: positional requests resolve against the
  -- acknowledged tree, so the new version must be recorded first.
  ack(params.version)
  refresh_tokens()
end

--- The cursor of the status window, 0-based.
local function current_cursor()
  local win = vim.api.nvim_get_current_win()
  if vim.api.nvim_win_get_buf(win) ~= M.buf then
    win = status_window()
  end
  if not win then
    return { line = 0, column = 0 }
  end
  local position = vim.api.nvim_win_get_cursor(win)
  return { line = position[1] - 1, column = position[2] }
end

--- The current visual selection, 0-based and inclusive.
local function visual_selection()
  local first, last = vim.fn.line('v'), vim.fn.line('.')
  if first == 0 or last == 0 then
    return nil
  end
  return {
    startLine = math.min(first, last) - 1,
    endLine = math.max(first, last) - 1,
  }
end

--- Run a command against the open view.
---@param command string
---@param opts table|nil { cursor?: table, selection?: boolean|table }
---@return table|nil response
function M.invoke(command, opts)
  opts = opts or {}
  if not M.view_id then
    return nil, 'no view is open'
  end

  local cursor = opts.cursor or current_cursor()
  local selection = opts.selection
  if selection == true then
    selection = visual_selection()
  end

  local before = M.version
  local resp, err = request_sync('view/invoke', {
    viewId = M.view_id,
    command = command,
    cursor = cursor,
    selection = selection,
  })
  if not resp then
    M.last_error_detail = type(err) == 'table' and err.data and err.data.detail or nil
    vim.notify('magit-lsp ' .. command .. ' failed: ' .. error_message(err), vim.log.levels.WARN)
    return nil, err
  end

  apply_effects(resp.effects)
  if M.version == before then
    vim.wait(5000, function()
      return M.version ~= before
    end, 10)
  end
  return resp
end

--- Open the status view in the current buffer.
---@return table|nil descriptor
function M.open()
  local descriptor, err = request_sync('view/open', {
    kind = 'status',
    args = { repo_path = M.root },
    viewport = {
      cols = vim.o.columns,
      rows = vim.o.lines,
    },
  })
  if not descriptor then
    vim.notify('magit-lsp open failed: ' .. error_message(err), vim.log.levels.ERROR)
    return nil, err
  end

  local existing = vim.fn.bufnr(descriptor.uri)
  if existing ~= -1 and existing ~= M.buf then
    vim.api.nvim_buf_delete(existing, { force = true })
  end

  M.view_id = descriptor.id
  M.uri = descriptor.uri
  M.version = nil
  vim.api.nvim_buf_set_name(M.buf, descriptor.uri)

  -- The initial render follows the response as a notification.
  if not vim.wait(5000, function()
    return M.version ~= nil
  end, 10) then
    vim.notify('magit-lsp did not render the view', vim.log.levels.ERROR)
    return nil, 'no initial render'
  end
  return descriptor
end

--- Ask the server to re-read the repository and re-render.
function M.refresh()
  return M.invoke('common.refresh')
end

--- Send the current viewport to the server.
function M.resize()
  local win = status_window()
  local client = M.client_id and vim.lsp.get_client_by_id(M.client_id)
  if not win or not client or not M.view_id then
    return
  end
  client:notify('view/resize', {
    viewId = M.view_id,
    cols = vim.api.nvim_win_get_width(win),
    rows = vim.api.nvim_win_get_height(win),
  })
end

--- LSP `SymbolKind` values for the server's section/file/hunk outline.
local SYMBOL_KINDS = { section = 3, file = 1, hunk = 6 }

--- Fetch the document's hierarchical symbol tree (section → file → hunk).
function M.symbols()
  local symbols, err = request_sync('textDocument/documentSymbol', {
    textDocument = { uri = document_uri() },
  })
  if not symbols then
    return nil, err
  end
  return symbols
end

local function collect_symbol_lines(symbols, kind, lines)
  for _, symbol in ipairs(symbols or {}) do
    if symbol.kind == kind then
      table.insert(lines, symbol.range.start.line + 1)
    end
    collect_symbol_lines(symbol.children, kind, lines)
  end
end

--- Move the cursor to the next/previous symbol of `kind` (`dir` is 1 or -1).
function M.jump(kind, dir)
  local symbols, err = M.symbols()
  if not symbols then
    return nil, err
  end
  local wanted = SYMBOL_KINDS[kind]
  if not wanted then
    return nil, 'unknown symbol kind: ' .. tostring(kind)
  end
  local lines = {}
  collect_symbol_lines(symbols, wanted, lines)
  table.sort(lines)

  local cur = vim.fn.line('.')
  if dir > 0 then
    for _, line in ipairs(lines) do
      if line > cur then
        vim.api.nvim_win_set_cursor(0, { line, 0 })
        return line
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
  return nil, 'no ' .. kind .. ' symbol'
end

--- Visit the file (and new-side line) under the cursor.
--- Opens it in the window the status view was split from, so the status
--- buffer stays visible.
function M.visit()
  local win = vim.api.nvim_get_current_win()
  vim.lsp.buf.definition({
    on_list = function(options)
      local item = options.items[1]
      if not item or not item.filename then
        return
      end
      if vim.api.nvim_get_current_win() == win then
        vim.cmd('wincmd p')
      end
      vim.cmd('edit ' .. vim.fn.fnameescape(item.filename))
      vim.api.nvim_win_set_cursor(0, { item.lnum, math.max((item.col or 1) - 1, 0) })
    end,
  })
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

local function invoke(command, opts)
  return M.invoke(command, opts)
end

local function keymaps()
  local opts = { buffer = M.buf, nowait = true, noremap = true, silent = true }
  vim.keymap.set('n', 's', function()
    invoke('status.stage')
  end, vim.tbl_extend('force', opts, { desc = 'Stage at cursor' }))
  vim.keymap.set('n', 'u', function()
    invoke('status.unstage')
  end, vim.tbl_extend('force', opts, { desc = 'Unstage at cursor' }))
  vim.keymap.set('x', 's', function()
    invoke('status.stage', { selection = true })
  end, vim.tbl_extend('force', opts, { desc = 'Stage selection' }))
  vim.keymap.set('x', 'u', function()
    invoke('status.unstage', { selection = true })
  end, vim.tbl_extend('force', opts, { desc = 'Unstage selection' }))
  vim.keymap.set('n', 'S', function()
    invoke('status.stage-all')
  end, vim.tbl_extend('force', opts, { desc = 'Stage all' }))
  vim.keymap.set('n', 'U', function()
    invoke('status.unstage-all')
  end, vim.tbl_extend('force', opts, { desc = 'Unstage all' }))
  vim.keymap.set('n', '<CR>', M.visit, vim.tbl_extend('force', opts, { desc = 'Visit file at line' }))
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
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].buftype = 'nofile'
  vim.bo[buf].bufhidden = 'hide'
  vim.bo[buf].swapfile = false
  vim.bo[buf].modifiable = false
  vim.bo[buf].readonly = true
  -- The view is server-owned and refresh replaces it wholesale.
  vim.bo[buf].undolevels = -1
  vim.bo[buf].modified = false
  -- A dedicated filetype keeps the `@lsp.type.*.<filetype>` group names stable.
  vim.bo[buf].filetype = M.filetype
  configure_read_only(buf)
  return buf
end

--- Start the server and open the status view.
---@param opts table|nil { root_dir?: string, cmd?: string[] }
---@return table|nil descriptor
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

  -- The server sends render updates as notifications.
  vim.lsp.handlers['view/render'] = function(_, result)
    M.apply_render(result)
  end

  M.buf = fresh_buffer()
  M.lines = {}
  M.version = nil
  M.view_id = nil
  M.uri = nil

  vim.cmd('vsplit')
  local win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, M.buf)
  enable_folding(win)

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

  vim.api.nvim_create_autocmd({ 'WinResized', 'VimResized' }, {
    group = vim.api.nvim_create_augroup('magit_lsp_resize', { clear = true }),
    callback = M.resize,
  })

  vim.api.nvim_create_autocmd('VimLeavePre', {
    callback = function()
      local client = M.client_id and vim.lsp.get_client_by_id(M.client_id)
      if client then
        client:stop(true)
      end
    end,
  })

  return M.open()
end

return M
