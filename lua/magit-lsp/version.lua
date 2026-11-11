--- Bundled Neovim adapter version and server mismatch reporting.
local M = { version = '0.1.0' }

--- Warn when initialized server metadata does not match this adapter.
---@param server_info table|nil
---@param command string[]
function M.check(server_info, command)
  local server_version = server_info and server_info.version
  if server_version == M.version then return end
  local executable = command[1] or 'unknown'
  if not executable:find('/', 1, true) then
    executable = vim.fn.exepath(executable) ~= '' and vim.fn.exepath(executable) or executable
  end
  vim.notify(string.format(
    'magit-lsp: client version %s, server version %s (executable: %s)',
    M.version, server_version or 'missing', executable
  ), vim.log.levels.WARN)
end

return M
