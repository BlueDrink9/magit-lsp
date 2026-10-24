--- User-facing effect primitives.
---
--- Effects that need a view (push/pop/focus) are dispatched by the view
--- manager; this module owns the client-side presentation of messages,
--- clipboard copies and native key replay, so `init.lua` stays small.

local keys = require('magit-lsp.keys')

local M = {}

--- Detail of the last error message, for `:MagitLspErrorDetail`.
M.last_error_detail = nil

local LEVELS = {
  info = vim.log.levels.INFO,
  warn = vim.log.levels.WARN,
  error = vim.log.levels.ERROR,
}

--- Show one message effect as a notification at its level.
---
--- Everything is flattened onto one line: a multi-line notification opens
--- Neovim's hit-enter prompt, which blocks the RPC loop until a key arrives.
---@param effect table
function M.notify(effect)
  M.last_error_detail = effect.detail
  local text = effect.text or ''
  if effect.hint then
    text = text .. ' - ' .. effect.hint
  end
  text = text:gsub('%s*\n%s*', ' ')
  -- A notification wider than the screen wraps and opens the hit-enter prompt,
  -- so keep a margin below `columns`.
  local limit = math.max(24, vim.o.columns - 12)
  if #text > limit then
    text = text:sub(1, limit - 3) .. '...'
  end
  vim.notify(text, LEVELS[effect.level] or vim.log.levels.INFO)
end

--- Put `text` on the clipboard and the unnamed register.
---
--- `+` has no provider in headless or remote setups, so a failure degrades to
--- the unnamed register and a warning instead of an error.
---@param text string
function M.copy(text)
  local ok = pcall(vim.fn.setreg, '+', text)
  pcall(vim.fn.setreg, '"', text)
  local preview = text:gsub('\n', ' ⏎ ')
  if #preview > 60 then
    preview = preview:sub(1, 57) .. '...'
  end
  if ok then
    vim.notify('Copied: ' .. preview, vim.log.levels.INFO)
  else
    vim.notify('Copied: ' .. preview .. ' (clipboard unavailable)', vim.log.levels.WARN)
  end
end

--- Replay a server `passThrough` key natively, without re-triggering our maps.
---@param key string canonical chord, e.g. `Escape` or `g Q`
function M.replay(key)
  local lhs = keys.to_lhs(key)
  if not lhs then
    return
  end
  -- `n` means "do not remap", so the client's own buffer-local map is skipped.
  vim.api.nvim_feedkeys(keys.termcodes(lhs), 'n', false)
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

return M
