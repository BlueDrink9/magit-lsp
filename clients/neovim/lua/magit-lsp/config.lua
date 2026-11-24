--- Client-only configuration and server discovery.
---
--- Nothing in this module is sent to the server except the key-delivery
--- capabilities. It owns option validation, the LSP client capabilities and
--- the binary lookup order, so `init.lua` only wires sessions and views.

local M = {}

local shared = require("magit-lsp.shared")

--- The status placements this client implements (R-UI-7).
M.PLACEMENTS = { "replace", "vsplit", "split", "tab" }

--- The rich protocol session version this client selects and requires.
M.PROTOCOL_VERSION = 1

--- The repository root containing this module, used for `target/` lookups.
---
--- The client runtime sits at `clients/neovim/lua/magit-lsp/`, so discovery
--- walks three levels above the client directory. An installed client has no
--- `target/` tree and falls through to the normal `$PATH` lookup.
---@return string
function M.project_root()
  local source = debug.getinfo(1, "S").source
  local path = source:sub(2)
  path = vim.fn.fnamemodify(path, ":h") -- lua/magit-lsp
  path = vim.fn.fnamemodify(path, ":h") -- lua
  path = vim.fn.fnamemodify(path, ":h") -- clients/neovim
  path = vim.fn.fnamemodify(path, ":h") -- clients
  return vim.fn.fnamemodify(path, ":h") -- repository root
end

--- Validate the client-only `status.placement` option.
---@param value any
---@return string name
function M.placement(value)
  if value == nil then
    return "replace"
  end
  for _, name in ipairs(M.PLACEMENTS) do
    if value == name then
      return name
    end
  end
  vim.notify(
    "magit-lsp: unknown status.placement " .. vim.inspect(value) .. "; using replace",
    vim.log.levels.WARN
  )
  return "replace"
end

--- True when `path` names an executable file.
---@param path string
---@return boolean
local function executable(path)
  return path ~= "" and vim.fn.executable(path) == 1
end

--- The server command to spawn.
---
--- The lookup order is an explicit `setup({ cmd = ... })`, `$MAGIT_LSP_BIN`,
--- `target/release/magit-lsp`, `target/debug/magit-lsp` and finally
--- `magit-lsp` on `$PATH`. There is deliberately no `cargo run` fallback: a
--- missing or unusable binary is a clear error instead of a surprise build.
---@param cmd string[]|function|nil the explicit command or transport from `setup()`
---@return string[]|function|nil command, string|nil error
function M.server_command(cmd)
  if cmd then
    return cmd
  end
  local env = vim.env.MAGIT_LSP_BIN
  if env and env ~= "" then
    if not executable(env) then
      return nil, "magit-lsp: $MAGIT_LSP_BIN is not executable: " .. env
    end
    return { env }
  end
  local root = M.project_root()
  for _, profile in ipairs({ "release", "debug" }) do
    local path = root .. "/target/" .. profile .. "/magit-lsp"
    if executable(path) then
      return { path }
    end
  end
  local found = vim.fn.exepath("magit-lsp")
  if found ~= "" then
    return { found }
  end
  return nil,
    "magit-lsp: server binary not found; set setup({ cmd = ... }) or "
      .. "$MAGIT_LSP_BIN, build target/debug/magit-lsp, or put magit-lsp on $PATH"
end

--- How faithfully this client can report key modifiers (§3.8).
---@return string
function M.key_modifiers()
  if vim.fn.has("gui_running") == 1 then
    return "full"
  end
  local program = vim.env.TERM_PROGRAM
  if program == "iTerm.app" or program == "WezTerm" or vim.env.KITTY_WINDOW_ID then
    return "full"
  end
  return "basic"
end

--- The client capabilities the server reads, plus the key-delivery contract.
---@return table
function M.client_capabilities()
  local capabilities = vim.lsp.protocol.make_client_capabilities()
  capabilities.experimental = capabilities.experimental or {}
  capabilities.experimental.magit = {
    v = M.PROTOCOL_VERSION,
    minimumVersion = M.PROTOCOL_VERSION,
    keys = "dynamic",
    modifiers = M.key_modifiers(),
    layout = { "main", "side", "bottom", "float" },
    effects = shared.optional_effects,
    prompts = shared.optional_prompt_kinds,
    pick = {
      multi = false,
      freeText = false,
      description = false,
      detail = false,
      group = false,
      preselection = false,
      maxItems = 100,
      fallbackView = true,
    },
  }
  return capabilities
end

return M
