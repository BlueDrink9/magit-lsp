-- Isolated Neovim init for the real lazy.nvim local-checkout installation check.
--
-- scripts/test_lazy_install.py supplies the isolated HOME/XDG tree, the pinned
-- lazy.nvim checkout, the adapted documented spec and the fixture repository.
-- This file is test plumbing: it is not a user configuration and it never
-- calls the documented key callback directly.

local function required_env(name)
  local value = vim.env[name]
  if not value or value == "" then
    io.stderr:write("lazy-install: missing environment variable " .. name .. "\n")
    vim.cmd("cquit 1")
  end
  return value
end

local checkout = required_env("MAGIT_LAZY_CHECKOUT")
local runtime = checkout .. "/clients/neovim"
local spec_path = required_env("MAGIT_LAZY_SPEC")
local manager_path = required_env("MAGIT_LAZY_MANAGER")
local evidence_path = required_env("MAGIT_LAZY_EVIDENCE")
local fixture = required_env("MAGIT_LAZY_FIXTURE")

--- Write one JSON evidence file, aborting the run if the path is unusable.
local function write_json(path, value)
  local file, error_message = io.open(path, "w")
  if not file then
    error("cannot write evidence " .. path .. ": " .. tostring(error_message), 0)
  end
  file:write(vim.json.encode(value))
  file:close()
end

--- Return whether a plugin root is present on Neovim's runtimepath.
local function on_runtimepath(path)
  for entry in vim.o.runtimepath:gmatch("[^,]+") do
    if entry == path then
      return true
    end
  end
  return false
end

--- Return the first attached magit-lsp client, if any.
local function magit_client()
  for _, client in ipairs(vim.lsp.get_clients()) do
    if client.name == "magit-lsp" then
      return client
    end
  end
end

--- Require a condition to hold or stop with one auditable message.
local function check(condition, message)
  if not condition then
    error(message, 0)
  end
end

--- Prove the documented spec, launcher and lifecycle state before any input.
local function pre_key_audit(spec)
  check(vim.fn.has("nvim-0.11") == 1, "pre-key oracle: Neovim 0.11+ required")
  check(spec.lazy == true, "pre-key oracle: documented spec is not lazy")
  check(spec.opts == nil, "pre-key oracle: documented spec sets opts")
  check(
    type(spec.config) == "function",
    "pre-key oracle: documented spec has no config(plugin) runtime hook"
  )
  check(spec.build == nil, "pre-key oracle: documented spec sets build")
  check(
    spec.dir == checkout,
    "pre-key oracle: adapted spec source is not this checkout: " .. tostring(spec.dir)
  )
  local mapping_spec = spec.keys and spec.keys[1]
  check(
    type(mapping_spec) == "table"
      and mapping_spec[1] == "<leader>gs"
      and type(mapping_spec[2]) == "function",
    "pre-key oracle: documented <leader>gs mapping changed"
  )
  check(
    package.loaded["magit-lsp"] == nil,
    "pre-key oracle: magit-lsp was preloaded or bypassed the lazy manager"
  )
  check(
    #vim.lsp.get_clients() == 0,
    "pre-key oracle: an LSP client started before the documented mapping was pressed"
  )
  local plugin = require("lazy.core.config").plugins["magit-lsp"]
  check(plugin ~= nil, "pre-key oracle: real lazy.nvim did not register the magit-lsp spec")
  check(plugin._.loaded == nil, "pre-key oracle: real lazy.nvim already loaded magit-lsp")

  local handler = require("lazy.core.handler").handlers.keys
  local parsed = require("lazy.core.handler.keys").parse("<leader>gs")
  check(
    handler.managed[parsed.id] == "magit-lsp",
    "pre-key oracle: lazy.nvim does not own the <leader>gs mapping"
  )
  local mapping = vim.fn.maparg("<leader>gs", "n", false, true)
  check(
    not vim.tbl_isempty(mapping) and mapping.desc == "Git status",
    "pre-key oracle: lazy.nvim did not install the documented <leader>gs mapping"
  )
  check(
    not on_runtimepath(runtime),
    "pre-key oracle: the client runtime is on runtimepath before the mapping was pressed"
  )
  check(
    vim.fn.exists(":Magit") == 0,
    "pre-key oracle: :Magit was registered before the manager loaded the plugin"
  )
  local root_runtime = vim.fn.isdirectory(checkout .. "/lua") == 1
    or vim.fn.isdirectory(checkout .. "/plugin") == 1
  check(
    not root_runtime,
    "pre-key oracle: the checkout root ships a Neovim runtime instead of clients/neovim"
  )
  return {
    plugin_loaded = package.loaded["magit-lsp"] ~= nil,
    lsp_clients = #vim.lsp.get_clients(),
    lazy_loaded = plugin._.loaded ~= nil,
    runtime_on_runtimepath = on_runtimepath(runtime),
    root_runtime_present = root_runtime,
    magit_command_available = vim.fn.exists(":Magit") == 2,
    mapping = {
      lhs = mapping.lhs,
      mode = mapping.mode,
      desc = mapping.desc,
      expr = mapping.expr,
      nowait = mapping.nowait,
    },
    lazy_key_owner = handler.managed[parsed.id],
  }
end

--- Wait for the LSP client to exist so cleanup ownership is recorded early.
local function wait_for_client()
  local ready = vim.wait(20000, function()
    return magit_client() ~= nil
  end, 50)
  check(ready, "documented mapping did not start the magit-lsp client")
  return magit_client()
end

--- Wait for the complete handshake and rendered status after the mapping fires.
local function wait_for_status()
  local ready = vim.wait(20000, function()
    local client = magit_client()
    if not client then
      return false
    end
    if client.initialized ~= true or client.server_capabilities == nil then
      return false
    end
    if vim.bo.filetype ~= "magit-lsp" then
      return false
    end
    local text = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
    return text:find(fixture, 1, true) ~= nil and text:find("Head:", 1, true) ~= nil
  end, 50)
  check(ready, "documented mapping did not load the plugin and render status")
end

--- Run the documented mapping through real input and collect evidence.
local function exercise(spec, pre_key)
  local mapping_spec = spec.keys[1]
  local keys = vim.api.nvim_replace_termcodes(mapping_spec[1], true, false, true)
  vim.api.nvim_feedkeys(keys, "mx", false)
  wait_for_client()
  wait_for_status()

  local client = magit_client()
  local plugin = require("lazy.core.config").plugins["magit-lsp"]
  local module = require("magit-lsp")
  local source = debug.getinfo(module.setup, "S").source:gsub("^@", "")
  local text = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")

  check(plugin._.loaded ~= nil, "post-key oracle: lazy.nvim did not mark the plugin loaded")
  check(
    source == checkout .. "/clients/neovim/lua/magit-lsp/init.lua",
    "post-key oracle: module loaded outside the client runtime: " .. source
  )
  check(
    on_runtimepath(runtime),
    "post-key oracle: the client runtime is not on runtimepath after loading"
  )
  check(
    vim.fn.exists(":Magit") == 2,
    "post-key oracle: the loaded nested plugin entry point did not register :Magit"
  )
  check(vim.bo.filetype == "magit-lsp", "post-key oracle: status buffer filetype changed")
  check(
    text:find("Untracked files", 1, true) ~= nil,
    "post-key oracle: status did not render the untracked fixture section"
  )

  local client_state = {
    name = client.name,
    initialized = client.initialized,
    server_capabilities = client.server_capabilities ~= nil,
    server_info = client.server_info,
  }

  local result = {
    pre_key = pre_key,
    post_key = {
      plugin_loaded = plugin._.loaded ~= nil,
      plugin_dir = plugin.dir,
      plugin_source = source,
      module_loaded = package.loaded["magit-lsp"] ~= nil,
      runtime_on_runtimepath = on_runtimepath(runtime),
      buffer = vim.api.nvim_get_current_buf(),
      buffer_name = vim.api.nvim_buf_get_name(0),
      filetype = vim.bo.filetype,
      magit_command_available = vim.fn.exists(":Magit") == 2,
      text = text,
      client = client_state,
    },
  }

  for _, current in ipairs(vim.lsp.get_clients()) do
    current:stop()
  end
  result.clients_stopped = vim.wait(5000, function()
    return #vim.lsp.get_clients() == 0
  end, 20)
  return result
end

--- Boot real lazy.nvim, audit the pre-key state, then press <leader>gs.
local function main()
  -- A negative control deliberately bypasses the manager; the pre-key oracle
  -- must reject the same preloaded state the positive run proves absent.
  if vim.env.MAGIT_LAZY_FORCE_PRELOAD == "1" then
    vim.opt.rtp:append(runtime)
    local ok, error_message = pcall(require, "magit-lsp")
    check(ok, "negative-control preload failed: " .. tostring(error_message))
  end

  vim.g.mapleader = " "
  vim.opt.rtp:prepend(manager_path)

  local ok, spec = pcall(dofile, spec_path)
  check(ok, "documented spec failed to load: " .. tostring(spec))

  local setup_ok, setup_error = pcall(function()
    require("lazy").setup({ spec }, {
      install = { missing = false },
      checker = { enabled = false },
      change_detection = { enabled = false },
    })
  end)
  check(setup_ok, "real lazy.nvim setup failed: " .. tostring(setup_error))

  local pre_key = pre_key_audit(spec)
  write_json(evidence_path, exercise(spec, pre_key))
end

vim.api.nvim_create_autocmd("VimEnter", {
  once = true,
  callback = function()
    vim.schedule(function()
      local ok, error_message = xpcall(main, debug.traceback)
      if not ok then
        io.stderr:write("lazy-install: " .. tostring(error_message) .. "\n")
        vim.cmd("cquit 1")
      else
        vim.cmd("qa!")
      end
    end)
  end,
})
