--- Git-owned edit documents from the editor bridge (§3.10).
---
--- The server asks the client to edit a real file (a commit message, a merge
--- message, a rebase todo) and keeps Git blocked until `common.confirm` or
--- `common.cancel` answers the edit id. This module opens the file in a bottom
--- split, applies the server's `commit-message` keymap and forwards the two
--- answers. Closing the buffer without confirming cancels the edit.

local keymap = require('magit-lsp.keymap')
local debug_api = require('magit-lsp.debug')

local M = {}

--- Open edits, keyed by edit id.
---@type table<string, table>
M.views = {}

--- Keymaps that arrived before their `editDocument` effect, by edit id.
---@type table<string, table[]>
M.pending_keymap = {}

--- Callbacks into the main client.
M.host = {}

--- The filetype for each edit kind; unknown kinds edit as a commit message.
local FILETYPES = {
  ['commit-message'] = 'gitcommit',
  ['merge-message'] = 'gitcommit',
  ['tag-message'] = 'gitcommit',
  ['rebase-todo'] = 'gitrebase',
}

--- Called by the main client once the LSP client exists.
---@param opts table { client_id, invoke, focus_status }
function M.configure(opts)
  for key, value in pairs(opts or {}) do
    M.host[key] = value
  end
end

local function client()
  if not M.host.client_id then
    return nil
  end
  return vim.lsp.get_client_by_id(M.host.client_id)
end

--- Whether `id` is an edit this module owns (open or waiting for its keymap).
---@param id string
function M.owns(id)
  return id ~= nil and (M.views[id] ~= nil or M.pending_keymap[id] ~= nil)
end

local function close(view)
  view.closing = true
  M.views[view.id] = nil
  M.pending_keymap[view.id] = nil
  if view.win and vim.api.nvim_win_is_valid(view.win) then
    pcall(vim.api.nvim_win_close, view.win, true)
  end
  if view.buf and vim.api.nvim_buf_is_valid(view.buf) then
    pcall(vim.api.nvim_buf_delete, view.buf, { force = true })
  end
  if M.host.focus_status then
    M.host.focus_status()
  end
end

local function answer(view, command)
  if M.host.invoke then
    M.host.invoke(command, { editId = view.id })
  end
  close(view)
end

--- Save the edited file and let Git continue.
---@param id string
function M.confirm(id)
  local view = M.views[id]
  if not view or view.closing then
    return
  end
  if view.buf and vim.api.nvim_buf_is_valid(view.buf) then
    pcall(vim.api.nvim_buf_call, view.buf, function()
      vim.cmd('silent write')
    end)
  end
  answer(view, 'common.confirm')
end

--- Abort the edit, leaving Git's operation undone.
---@param id string
function M.cancel(id)
  local view = M.views[id]
  if not view or view.closing then
    return
  end
  answer(view, 'common.cancel')
end

--- A buffer was wiped or deleted: unless we closed it, that means cancel.
---@param id string
function M.on_closed(id)
  local view = M.views[id]
  if not view or view.closing then
    return
  end
  M.cancel(id)
end

--- Apply a `view/keymap` for an edit; buffer it when the effect is late.
---@param notice table
function M.apply_keymap(notice)
  if type(notice) ~= 'table' or not notice.viewId then
    return
  end
  local view = M.views[notice.viewId]
  if not view then
    local queue = M.pending_keymap[notice.viewId] or {}
    queue[#queue + 1] = notice
    M.pending_keymap[notice.viewId] = queue
    return
  end
  view.notice = notice
  keymap.apply(view.buf, notice, {
    press = function() end,
    command = function() end,
  }, {
    folds = false,
    only_commands = true,
    commands = {
      ['common.confirm'] = function()
        M.confirm(view.id)
      end,
      ['common.cancel'] = function()
        M.cancel(view.id)
      end,
      -- `q`/quit in an edit buffer means "stop editing", i.e. cancel.
      ['common.quit'] = function()
        M.cancel(view.id)
      end,
    },
  })
  debug_api.keymap_received(view.id, notice)
end

--- Open the real file for an `editDocument` effect.
---@param effect table
---@return table|nil view
function M.open(effect)
  if type(effect) ~= 'table' or not effect.editId or not effect.path then
    return nil
  end
  local id = effect.editId
  local existing = M.views[id]
  if existing then
    if existing.win and vim.api.nvim_win_is_valid(existing.win) then
      vim.api.nvim_set_current_win(existing.win)
    end
    return existing
  end

  local buf = vim.fn.bufadd(effect.path)
  vim.fn.bufload(buf)
  local view = {
    id = id,
    buf = buf,
    effect = effect,
    closing = false,
  }
  M.views[id] = view

  vim.cmd('botright split')
  local win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, buf)
  view.win = win
  vim.bo[buf].filetype = FILETYPES[effect.kind] or 'gitcommit'
  vim.bo[buf].swapfile = false

  local c = client()
  if c and vim.lsp.buf_attach_client then
    pcall(vim.lsp.buf_attach_client, buf, c.id)
  end

  vim.api.nvim_create_autocmd({ 'BufWipeout', 'BufDelete' }, {
    buffer = buf,
    callback = function()
      M.on_closed(id)
    end,
  })

  debug_api.trace('edit_open', { edit = id, kind = effect.kind, path = effect.path })

  local pending = M.pending_keymap[id]
  M.pending_keymap[id] = nil
  for _, notice in ipairs(pending or {}) do
    M.apply_keymap(notice)
  end
  return view
end

--- Handle the `editDocument` effects of one response or notification.
---@param effects table[]
function M.apply_effects(effects)
  for _, effect in ipairs(effects or {}) do
    if effect.type == 'editDocument' then
      M.open(effect)
    end
  end
end

--- Close every edit buffer, e.g. on `VimLeavePre`.
function M.close_all()
  local open = {}
  for _, view in pairs(M.views) do
    open[#open + 1] = view
  end
  for _, view in ipairs(open) do
    view.closing = true
    close(view)
  end
end

return M
