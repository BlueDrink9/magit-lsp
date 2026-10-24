--- Git-owned edit documents from the editor bridge (§3.10).
---
--- The server asks the client to edit a real file (a commit message, a merge
--- message, a rebase todo) and keeps Git blocked until `common.confirm` or
--- `common.cancel` answers the edit id. This module contributes only the
--- `edit` placement and the two command handlers; opening, the pending keymap
--- queue and closing are the shared view manager's job.

local view = require('magit-lsp.view')

local M = {}

--- The filetype for each edit kind; unknown kinds edit as a commit message.
local FILETYPES = {
  ['commit-message'] = 'gitcommit',
  ['merge-message'] = 'gitcommit',
  ['tag-message'] = 'gitcommit',
  ['rebase-todo'] = 'gitrebase',
}

--- Open the real file of an edit document in a bottom split.
--- `view.open_document` calls this before it installs the close autocmds.
---@param record table the document record
---@return integer win
function M.place(record)
  local buf = vim.fn.bufadd(record.uri)
  vim.fn.bufload(buf)
  vim.bo[buf].swapfile = false
  record.buf = buf

  vim.cmd('botright split')
  local win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, buf)
  return win
end

--- Register the `edit` placement.
function M.setup()
  view.register_placement('edit', { place = M.place })
end

--- Save the edited file and let Git continue.
---@param key string local view key
function M.confirm(key)
  local record = view.views[key]
  if not record or record.closing then
    return
  end
  if record.buf and vim.api.nvim_buf_is_valid(record.buf) then
    local ok, err = pcall(vim.api.nvim_buf_call, record.buf, function()
      vim.cmd('silent write')
    end)
    if not ok then
      vim.notify('magit-lsp: could not save edit document: ' .. tostring(err), vim.log.levels.ERROR)
      return
    end
  end
  M.answer(record, 'common.confirm')
end

--- Abort the edit, leaving Git's operation undone.
---@param key string local view key
function M.cancel(key)
  local record = view.views[key]
  if not record or record.closing then
    return
  end
  M.answer(record, 'common.cancel')
end

--- A real-file buffer was wiped or deleted: unless we closed it, cancel.
---@param record table the document record
function M.on_closed(record)
  if not record or record.closing then
    return
  end
  M.answer(record, 'common.cancel')
end

--- Answer Git on the base view of the same session, then close the document.
---@param record table
---@param command string `common.confirm` or `common.cancel`
function M.answer(record, command)
  local base = record.base
  view.close(record.key, false)
  if base and view.views[base] then
    view.invoke(command, { args = { editId = record.id } }, base)
    view.focus(base)
  end
end

--- Key handling for an edit: only the server's confirm/cancel bindings apply,
--- so the real file keeps its ordinary editor keys.
---@param record table
---@return table handlers, table opts
local function keymap_options(record)
  return {
    press = function() end,
    command = function() end,
  }, {
    folds = false,
    only_commands = true,
    commands = {
      ['common.confirm'] = function()
        M.confirm(record.key)
      end,
      ['common.cancel'] = function()
        M.cancel(record.key)
      end,
      -- `q`/quit in an edit buffer means "stop editing", i.e. cancel.
      ['common.quit'] = function()
        M.cancel(record.key)
      end,
    },
  }
end

--- Open the real file for an `editDocument` effect.
---@param effect table `editDocument` effect
---@param source table|nil the server view whose command produced it
---@return table|nil record
function M.open(effect, source)
  if type(effect) ~= 'table' or not effect.editId or not effect.path then
    return nil
  end
  source = source and source.key and source or view.last_for_client(source and source.client_id)
  local base = source and (source.document and source.base or source.key) or nil
  return view.open_document(effect, {
    placement = 'edit',
    filetype = FILETYPES[effect.kind] or 'gitcommit',
    client_id = source and source.client_id or nil,
    root = source and source.root or nil,
    base = base,
    keymap_options = keymap_options,
    on_closed = M.on_closed,
  })
end

return M
