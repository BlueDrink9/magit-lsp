--- Menu views: bottom placement, per-view buffers and effects (§3.11).
---
--- The server owns menu text, layout and key handling. This module only
--- creates a scratch buffer and window for each menu descriptor, applies the
--- server's renders/keymaps, and forwards keys through `view/key`.

local keymap = require('magit-lsp.keymap')

local M = {}

--- Open menu views, keyed by view id.
---@type table<string, table>
M.views = {}

--- The menu stack, oldest first.
---@type string[]
M.order = {}

--- Renders/keymaps that arrived before their descriptor's effect.
M.pending_render = {}
M.pending_keymap = {}

--- `float` (default) or `split`.
M.style = 'float'

--- Callbacks into the main client.
M.host = {}

--- The id of the most recently opened menu.
M.last_id = nil

--- Called by the main client once the LSP client exists.
---@param opts table { client_id, filetype, notify, clipboard, replay, status_win }
function M.configure(opts)
  for key, value in pairs(opts) do
    M.host[key] = value
  end
end

--- Configure menu placement.
---@param opts table|nil { style?: 'float'|'split' }
function M.setup(opts)
  opts = opts or {}
  M.style = opts.style or 'float'
  vim.api.nvim_create_autocmd({ 'WinResized', 'VimResized' }, {
    group = vim.api.nvim_create_augroup('magit_lsp_menu_resize', { clear = true }),
    callback = function()
      for id in pairs(M.views) do
        M.resize(id)
      end
    end,
  })
end

--- Whether `id` belongs to this module (open or waiting for its first render).
---@param id string
function M.owns(id)
  return id ~= nil
    and (M.views[id] ~= nil or M.pending_render[id] ~= nil or M.pending_keymap[id] ~= nil)
end

local function client()
  if not M.host.client_id then
    return nil
  end
  return vim.lsp.get_client_by_id(M.host.client_id)
end

local function cap()
  return math.max(1, math.floor(vim.o.lines * 0.4))
end

local function height_for(lines)
  return math.max(1, math.min(#lines, cap()))
end

local function set_lines(buf, start, finish, lines)
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, start, finish, false, lines)
  vim.bo[buf].modified = false
  vim.bo[buf].modifiable = false
end

local function new_buffer(descriptor)
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].buftype = 'nofile'
  vim.bo[buf].bufhidden = 'wipe'
  vim.bo[buf].swapfile = false
  vim.bo[buf].modifiable = false
  vim.bo[buf].readonly = true
  vim.bo[buf].undolevels = -1
  vim.bo[buf].modified = false
  vim.bo[buf].filetype = M.host.filetype or 'magit-lsp'
  pcall(vim.api.nvim_buf_set_name, buf, descriptor.uri)
  return buf
end

local function focus_status()
  local win = M.host.status_win and M.host.status_win() or nil
  if win and vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_set_current_win(win)
    return true
  end
  return false
end

--- Focus a menu view, or the status view when it is gone.
---@param id string|nil
function M.focus(id)
  local view = id and M.views[id] or nil
  if view and view.win and vim.api.nvim_win_is_valid(view.win) then
    vim.api.nvim_set_current_win(view.win)
    return
  end
  focus_status()
end

local function focus_previous()
  local id = M.order[#M.order]
  if id then
    M.focus(id)
  else
    focus_status()
  end
end

local function forget(id)
  M.views[id] = nil
  M.pending_render[id] = nil
  M.pending_keymap[id] = nil
  for index, value in ipairs(M.order) do
    if value == id then
      table.remove(M.order, index)
      break
    end
  end
  M.last_id = M.order[#M.order]
end

--- A menu window was closed outside our own pop handling.
local function on_wiped(id)
  if M.closing and M.closing[id] then
    return
  end
  if not M.views[id] then
    return
  end
  forget(id)
  local c = client()
  if c then
    c:notify('view/close', { viewId = id })
  end
  focus_previous()
end

--- Re-fit a menu window to its content without telling the server.
---@param id string
function M.fit(id)
  local view = M.views[id]
  if not view or not view.win or not vim.api.nvim_win_is_valid(view.win) then
    return
  end
  local height = height_for(view.lines)
  if M.style == 'split' then
    pcall(vim.api.nvim_win_set_height, view.win, height)
  else
    local row = vim.o.lines - height - vim.o.cmdheight - 1
    vim.api.nvim_win_set_config(view.win, {
      relative = 'editor',
      row = math.max(0, row),
      col = 0,
      width = vim.o.columns,
      height = height,
    })
  end
end

--- Send the menu's viewport to the server when it actually changed.
---@param id string
function M.resize(id)
  local view = M.views[id]
  if not view or not view.win or not vim.api.nvim_win_is_valid(view.win) then
    return
  end
  M.fit(id)
  local cols = vim.api.nvim_win_get_width(view.win)
  local rows = vim.api.nvim_win_get_height(view.win)
  if view.cols == cols and view.rows == rows then
    return
  end
  view.cols = cols
  view.rows = rows
  local c = client()
  if c then
    c:notify('view/resize', { viewId = id, cols = cols, rows = rows })
  end
end

--- Open (or focus) the window for a menu descriptor.
---@param descriptor table `viewDescriptor` from a pushView/openView effect
---@return table|nil view
function M.open(descriptor)
  if not descriptor or not descriptor.id then
    return nil
  end
  local existing = M.views[descriptor.id]
  if existing then
    M.focus(descriptor.id)
    return existing
  end

  local buf = new_buffer(descriptor)
  local view = {
    id = descriptor.id,
    buf = buf,
    descriptor = descriptor,
    version = nil,
    lines = {},
    notice = nil,
  }
  M.views[descriptor.id] = view
  M.order[#M.order + 1] = descriptor.id
  M.last_id = descriptor.id

  local pending = M.pending_render[descriptor.id]
  if pending and pending.text then
    view.lines = pending.text
  end
  local height = height_for(view.lines)

  if M.style == 'split' then
    vim.cmd('botright ' .. height .. 'split')
    view.win = vim.api.nvim_get_current_win()
    vim.api.nvim_win_set_buf(view.win, buf)
  else
    local row = vim.o.lines - height - vim.o.cmdheight - 1
    view.win = vim.api.nvim_open_win(buf, true, {
      relative = 'editor',
      row = math.max(0, row),
      col = 0,
      width = vim.o.columns,
      height = height,
      style = 'minimal',
      border = 'none',
    })
  end
  vim.wo[view.win].foldmethod = 'manual'
  vim.wo[view.win].foldenable = false
  vim.wo[view.win].wrap = false

  local c = client()
  if c and vim.lsp.buf_attach_client then
    pcall(vim.lsp.buf_attach_client, buf, c.id)
  end

  vim.api.nvim_create_autocmd('BufWipeout', {
    buffer = buf,
    callback = function()
      on_wiped(descriptor.id)
    end,
  })

  local render = M.pending_render[descriptor.id]
  M.pending_render[descriptor.id] = nil
  if render then
    M.apply_render(render)
  end
  local notice = M.pending_keymap[descriptor.id]
  M.pending_keymap[descriptor.id] = nil
  if notice then
    M.apply_keymap(notice)
  end

  M.resize(descriptor.id)
  return view
end

--- Close a menu window and optionally return focus to the view below it.
---@param id string
---@param focus_previous boolean|nil
function M.close(id, focus_previous_window)
  local view = M.views[id]
  if not view then
    return
  end
  M.closing = M.closing or {}
  M.closing[id] = true
  forget(id)
  if view.win and vim.api.nvim_win_is_valid(view.win) then
    pcall(vim.api.nvim_win_close, view.win, true)
  end
  if view.buf and vim.api.nvim_buf_is_valid(view.buf) then
    pcall(vim.api.nvim_buf_delete, view.buf, { force = true })
  end
  M.closing[id] = nil
  if focus_previous_window then
    focus_previous()
  end
end

--- Close every menu, e.g. on VimLeavePre.
function M.close_all()
  for id in pairs(M.views) do
    M.close(id, false)
  end
end

local function apply_edits(buf, edits)
  table.sort(edits, function(a, b)
    return a.startLine > b.startLine
  end)
  for _, edit in ipairs(edits) do
    set_lines(buf, edit.startLine, edit.endLine, edit.lines)
  end
end

local function resync(id)
  local view = M.views[id]
  local c = client()
  if not view or not c then
    return
  end
  c:request('view/resync', { viewId = id }, function(err, result)
    if err or type(result) ~= 'table' then
      return
    end
    M.apply_render(result)
  end, view.buf)
end

--- Apply a `view/render` for a menu, then acknowledge it.
---@param params table
function M.apply_render(params)
  if not params or not params.viewId then
    return
  end
  local view = M.views[params.viewId]
  if not view then
    M.pending_render[params.viewId] = params
    return
  end

  if params.text then
    set_lines(view.buf, 0, -1, params.text)
  elseif params.edits then
    if params.baseVersion ~= view.version then
      resync(params.viewId)
      return
    end
    apply_edits(view.buf, params.edits)
  end

  view.lines = vim.api.nvim_buf_get_lines(view.buf, 0, -1, false)
  view.version = params.version
  M.fit(params.viewId)
  local c = client()
  if c then
    c:notify('view/rendered', { viewId = params.viewId, version = params.version })
  end
  vim.lsp.semantic_tokens.force_refresh(view.buf)
end

--- Apply a `view/keymap` for a menu.
---@param notice table
function M.apply_keymap(notice)
  if not notice or not notice.viewId then
    return
  end
  local view = M.views[notice.viewId]
  if not view then
    M.pending_keymap[notice.viewId] = notice
    return
  end
  view.notice = notice
  keymap.apply(view.buf, notice, {
    press = function(steps)
      M.press(notice.viewId, steps)
    end,
    command = function(command, args)
      M.invoke(notice.viewId, command, args)
    end,
  }, { fold_keys = {} })
end

--- Send one key chord to a menu and apply the answer.
---@param id string
---@param steps string[]
function M.press(id, steps)
  local view = M.views[id]
  local c = client()
  if not view or not c then
    return
  end
  local cursor = { line = 0, column = 0 }
  if view.win and vim.api.nvim_win_is_valid(view.win) then
    local position = vim.api.nvim_win_get_cursor(view.win)
    cursor = { line = position[1] - 1, column = position[2] }
  end

  local effects = {}
  for _, step in ipairs(steps) do
    local responses = vim.lsp.buf_request_sync(view.buf, 'view/key', {
      viewId = id,
      version = view.version or 0,
      key = step,
      editorMode = 'normal',
      cursor = cursor,
    }, 5000)
    if not responses then
      vim.notify('magit-lsp menu key ' .. step .. ' timed out', vim.log.levels.WARN)
      return
    end
    for _, response in pairs(responses) do
      if response.err then
        vim.notify(
          'magit-lsp menu key ' .. step .. ' failed: ' .. tostring(response.err.message or response.err),
          vim.log.levels.WARN
        )
      elseif response.result then
        vim.list_extend(effects, response.result.effects or {})
      end
    end
  end
  M.apply_effects(effects)
end

--- Run a command against a menu and apply the answer.
---@param id string
---@param command string
---@param args table|nil
function M.invoke(id, command, args)
  local view = M.views[id]
  local c = client()
  if not view or not c then
    return
  end
  local cursor = { line = 0, column = 0 }
  if view.win and vim.api.nvim_win_is_valid(view.win) then
    local position = vim.api.nvim_win_get_cursor(view.win)
    cursor = { line = position[1] - 1, column = position[2] }
  end
  local responses = vim.lsp.buf_request_sync(view.buf, 'view/invoke', {
    viewId = id,
    command = command,
    cursor = cursor,
    args = args,
  }, 5000)
  if not responses then
    return
  end
  local effects = {}
  for _, response in pairs(responses) do
    if response.err then
      vim.notify(
        'magit-lsp ' .. command .. ' failed: ' .. tostring(response.err.message or response.err),
        vim.log.levels.WARN
      )
    elseif response.result then
      vim.list_extend(effects, response.result.effects or {})
    end
  end
  M.apply_effects(effects)
end

--- Apply effects produced by a menu key or command.
---@param effects table[]
function M.apply_effects(effects)
  for _, effect in ipairs(effects or {}) do
    local kind = effect.type
    if kind == 'message' then
      if M.host.notify then
        M.host.notify(effect)
      end
    elseif kind == 'clipboard' then
      if M.host.clipboard then
        M.host.clipboard(effect.text or '')
      end
    elseif kind == 'passThrough' then
      if M.host.replay then
        M.host.replay(effect.key)
      end
    elseif kind == 'pushView' or kind == 'openView' then
      M.open(effect.descriptor)
    elseif kind == 'popView' then
      M.close(effect.viewId, true)
    elseif kind == 'focusView' then
      M.focus(effect.viewId)
    end
  end
end

return M
