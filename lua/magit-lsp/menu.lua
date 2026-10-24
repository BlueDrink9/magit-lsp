--- Menu placement: the `bottom` descriptor placement (§3.11).
---
--- Menus are ordinary server views; this module only decides where they are
--- shown and how tall they are. Render, keymap, key, invoke, resync, ack and
--- the pending queues all live in `view.lua`.

local view = require('magit-lsp.view')

local M = {}

--- `float` (default) or `split`.
M.style = 'float'

--- Height available to a menu: the whole editor above the command line. A
--- menu taller than this scrolls in its window instead of hiding its actions
--- below the float. A split also needs room for both windows' status lines.
---@return integer
local function cap()
  local reserved = M.style == 'split' and 3 or 1
  return math.max(1, vim.o.lines - vim.o.cmdheight - reserved)
end

--- The height a menu with `lines` lines asks for.
---@param lines string[]
---@return integer
local function height_for(lines)
  return math.max(1, math.min(#lines, cap()))
end

local function window_options(win)
  -- The server owns section visibility; native folding must never interfere.
  vim.wo[win].foldmethod = 'manual'
  vim.wo[win].foldenable = false
  vim.wo[win].wrap = false
end

--- Show a menu view at the bottom of the editor.
---@param record table the view record created by `view.open`
---@return integer win
function M.place(record)
  local height = height_for(record.lines or {})
  if M.style == 'split' then
    vim.cmd('botright ' .. height .. 'split')
    local win = vim.api.nvim_get_current_win()
    vim.api.nvim_win_set_buf(win, record.buf)
    window_options(win)
    -- The buffer is displayed now, so `wipe` is safe and makes closing the
    -- window close the view, exactly like the old menu manager.
    vim.bo[record.buf].bufhidden = 'wipe'
    return win
  end

  local row = vim.o.lines - height - vim.o.cmdheight - 1
  local win = vim.api.nvim_open_win(record.buf, true, {
    relative = 'editor',
    row = math.max(0, row),
    col = 0,
    width = vim.o.columns,
    height = height,
    style = 'minimal',
    border = 'none',
  })
  window_options(win)
  vim.bo[record.buf].bufhidden = 'wipe'
  return win
end

--- Re-fit a menu window to its current content without telling the server.
---@param record table
function M.fit(record)
  local win = record.win
  if not win or not vim.api.nvim_win_is_valid(win) then
    return
  end
  local height = height_for(record.lines or {})
  if M.style == 'split' then
    pcall(vim.api.nvim_win_set_height, win, height)
  else
    local row = vim.o.lines - height - vim.o.cmdheight - 1
    pcall(vim.api.nvim_win_set_config, win, {
      relative = 'editor',
      row = math.max(0, row),
      col = 0,
      width = vim.o.columns,
      height = height,
    })
  end
end

--- Register the `bottom` placement.
---@param opts table|nil { style?: 'float'|'split' }
function M.setup(opts)
  opts = opts or {}
  M.style = opts.style or 'float'
  view.register_placement('bottom', { place = M.place, fit = M.fit })
end

return M
