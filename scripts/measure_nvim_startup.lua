-- Run inside an isolated Neovim process; all evidence is structured text.
local root = vim.env.MAGIT_STARTUP_ROOT
local output = vim.env.MAGIT_STARTUP_OUTPUT
vim.opt.rtp:prepend(vim.env.MAGIT_STARTUP_REPO)
if vim.env.MAGIT_LSP_TRACE then
  require("magit-lsp.debug")
end
local function visible()
  if not vim.api.nvim_buf_is_valid(0) then
    return false
  end
  return table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n"):find("Head:", 1, true)
    ~= nil
end
local function record(case, started)
  local buf = vim.api.nvim_get_current_buf()
  local text = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), "\n")
  local win = vim.api.nvim_get_current_win()
  local ok = text:find("Head:", 1, true) ~= nil
    and vim.api.nvim_win_is_valid(win)
    and vim.api.nvim_win_get_buf(win) == buf
  local f = assert(io.open(output, "a"))
  f:write(vim.json.encode({
    case = case,
    elapsed_ms = (vim.uv.hrtime() - started) / 1e6,
    valid_status = ok,
    window = win,
    text = text,
  }) .. "\n")
  f:close()
  if not ok then
    vim.cmd.cquit()
  end
end

vim.cmd("runtime plugin/magit-lsp.lua")
local start = vim.uv.hrtime()
vim.cmd("Magit " .. vim.fn.fnameescape(root))
local opened = vim.wait(30000, visible, 10)
if not opened then
  error("timed out waiting for :Magit displayed status")
end
record("process_cold_first_open", start)
local old_buf = vim.api.nvim_get_current_buf()
vim.api.nvim_buf_delete(old_buf, { force = true })
local warm = vim.uv.hrtime()
vim.cmd("Magit " .. vim.fn.fnameescape(root))
if not vim.wait(30000, visible, 10) then
  error("timed out waiting for initialized-server reopen")
end
record("initialized_server_reopen", warm)
local focused = vim.uv.hrtime()
vim.cmd("Magit " .. vim.fn.fnameescape(root))
record("already_open_focus", focused)
for _, client in ipairs(vim.lsp.get_clients()) do
  client.stop(true)
end
vim.wait(3000, function()
  return #vim.lsp.get_clients() == 0
end, 10)
vim.cmd("qa!")
