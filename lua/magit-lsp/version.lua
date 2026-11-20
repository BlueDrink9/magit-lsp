--- Bundled Neovim adapter version and server mismatch reporting.
local M = { version = "0.2.0" }

--- Warn when initialized server metadata does not match this adapter.
---@param server_info table|nil
---@param command string[]|function
function M.check(server_info, command)
  local server_version = server_info and server_info.version
  if server_version == M.version then
    return
  end
  local executable = type(command) == "table" and command[1] or "custom transport"
  if not executable:find("/", 1, true) then
    executable = vim.fn.exepath(executable) ~= "" and vim.fn.exepath(executable) or executable
  end
  vim.notify(
    string.format(
      "magit-lsp: client version %s, server version %s (executable: %s)",
      M.version,
      server_version or "missing",
      executable
    ),
    vim.log.levels.WARN
  )
end

--- Report an incompatible rich protocol advertisement, if any.
---
--- The advertisement is the version of every accepted rich session, so it must
--- be a positive integer equal to the session version this client declared. A
--- missing, malformed or different value is a compatibility failure: there is
--- no fallback, default or retry.
---@param result any decoded initialize result; a non-table container is malformed
---@param required integer the rich protocol session version this client declared
---@return string|nil error readable mismatch naming required and advertised versions
function M.protocol_error(result, required)
  -- Every container comes from decoded JSON: a boolean, number or JSON null
  -- where an object belongs is malformed input, not an indexing error.
  local function member(container, key)
    if type(container) ~= "table" or container == vim.NIL then
      return nil
    end
    return container[key]
  end
  local capabilities = member(result, "capabilities")
  local experimental = member(capabilities, "experimental")
  local magit = member(experimental, "magit")
  local advertised = member(magit, "protocolVersion")
  if
    type(advertised) == "number"
    and advertised > 0
    and advertised % 1 == 0
    and advertised == required
  then
    return nil
  end
  local shown
  if advertised == nil then
    shown = "none"
  elseif type(advertised) == "number" and advertised % 1 == 0 then
    shown = tostring(advertised)
  else
    shown = vim.inspect(advertised)
  end
  return string.format(
    "magit-lsp: rich protocol mismatch: required version %d, server advertised %s",
    required,
    shown
  )
end

return M
