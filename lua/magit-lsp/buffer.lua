--- Buffer primitives for server-owned views.
---
--- Every view is displayed in a scratch buffer whose text only the server
--- writes. This module owns the read-only contract (TASKS.md IMM-4), the
--- render writer, text mode and native folding. It never decides layout or
--- interprets a view; the view manager passes a buffer number.

local M = {}

--- The filetype every view buffer uses; LSP groups are
--- `@lsp.type.<type>.<filetype>`.
M.filetype = 'magit-lsp'

--- The user's undo setting captured when each block buffer was created.
---@type table<integer, integer>
M.undo = {}

--- Whether the one-time read-only hint was shown.
M.read_only_hint_shown = false

--- The editor's undo setting for `buf`, captured before the view disabled it.
---@param buf integer
---@return integer
function M.undo_levels(buf)
  return M.undo[buf] or -1
end

local WRITE_COMMANDS = { w = true, wr = true, wri = true, writ = true, write = true }

--- True when `cmdline` is a bare `:w`/`:write` (any abbreviation, optional `!`).
---@param cmdline string
---@return boolean
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
  vim.notify(
    'Read-only view. Press the text-mode key (ctrl+t) to edit or copy as plain text.',
    vim.log.levels.INFO
  )
end

--- Keep a block buffer unmodified on `:w` and explain why.
--- `nofile` buffers reject `:w` with E382 before `BufWriteCmd` runs, so also
--- swallow a bare write at the command line.
---@param buf integer
local function configure_read_only(buf)
  vim.api.nvim_create_autocmd('BufWriteCmd', {
    buffer = buf,
    callback = function()
      vim.bo[buf].modified = false
      notify_read_only()
    end,
  })
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

--- Create a configured, read-only scratch buffer for a server view.
---@param name string|nil optional virtual buffer name
---@return integer buf
function M.fresh(name)
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].buftype = 'nofile'
  vim.bo[buf].bufhidden = 'hide'
  vim.bo[buf].swapfile = false
  vim.bo[buf].modifiable = false
  -- `readonly` is deliberately unset: Vim warns W10 on programmatic writes.
  M.undo[buf] = vim.o.undolevels
  vim.bo[buf].undolevels = -1
  vim.bo[buf].modified = false
  vim.bo[buf].filetype = M.filetype
  configure_read_only(buf)
  if name then
    pcall(vim.api.nvim_buf_set_name, buf, name)
  end
  return buf
end

--- Give a boot buffer its final server URI and keep it read-only.
---@param buf integer
---@param name string
function M.rename(buf, name)
  pcall(vim.api.nvim_buf_set_name, buf, name)
end

--- The only writer of a view buffer.
---@param buf integer
---@param start integer 0-based first line
---@param finish integer 0-based end line, exclusive; -1 means the last line
---@param lines string[]
---@param text_mode boolean whether the user currently owns the buffer
function M.set_lines(buf, start, finish, lines, text_mode)
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, start, finish, false, lines)
  vim.bo[buf].modified = false
  -- A stray render in text mode must not take the buffer back from the user.
  vim.bo[buf].modifiable = text_mode == true
end

--- Apply line-range edits bottom-up so earlier offsets stay valid.
---@param buf integer
---@param edits table[]
---@param text_mode boolean
function M.apply_edits(buf, edits, text_mode)
  table.sort(edits, function(a, b)
    return a.startLine > b.startLine
  end)
  for _, edit in ipairs(edits) do
    M.set_lines(buf, edit.startLine, edit.endLine, edit.lines, text_mode)
  end
end

--- Enter text mode: the buffer becomes plain editable text.
---@param buf integer
function M.enter_text_mode(buf)
  vim.bo[buf].modifiable = true
  vim.bo[buf].undolevels = M.undo[buf] or -1
end

--- Leave text mode: drop the undo history and hand the buffer back.
--- `undolevels=-1` is set before the server's text replaces the buffer, so `u`
--- cannot restore what the user typed while in text mode.
---@param buf integer
function M.exit_text_mode(buf)
  vim.bo[buf].undolevels = -1
  vim.bo[buf].modifiable = false
  vim.bo[buf].modified = false
end

--- Keep native folding out of the server-owned section visibility.
---@param win integer
function M.enable_folding(win)
  vim.wo[win].foldmethod = 'manual'
  vim.wo[win].foldenable = false
end

return M
