-- Real Mason registry/compiler/installer and installed-server lifecycle smoke.

--- Require one contract assertion with a readable failure.
local function check(condition, message)
  if not condition then
    error(message, 0)
  end
end

--- Read one required harness setting.
local function env(name)
  local value = vim.env["MAGIT_MASON_" .. name]
  check(value and value ~= "", "missing setting " .. name)
  return value
end

--- Write structured, non-image evidence.
local function write(value)
  local file = assert(io.open(env("EVIDENCE"), "w"))
  file:write(vim.json.encode(value))
  file:close()
end

--- Exercise actual Mason installation, without replacing its compiler or unpacker.
local function main()
  vim.opt.rtp:prepend(env("MANAGER"))
  require("mason").setup({
    install_root_dir = env("ROOT"),
    registries = { env("REGISTRY") },
    github = { download_url_template = env("URL") },
  })
  local registry = require("mason-registry")
  registry.refresh()
  local pkg = registry.get_package("magit-lsp")
  local done, success, result = false, nil, nil
  local output = {}
  local handle = pkg:install({}, function(ok, value)
    success, result, done = ok, tostring(value), true
  end)
  handle:on("stdout", function(chunk)
    output[#output + 1] = chunk
  end)
  handle:on("stderr", function(chunk)
    output[#output + 1] = chunk
  end)
  check(
    vim.wait(60000, function()
      return done
    end, 20),
    "Mason install callback timed out"
  )
  local text = table.concat(output)
  if env("MODE") == "negative" then
    check(success == false, "corrupt archive unexpectedly installed")
    check(
      result:match("spawn: g?tar failed with exit code [1-9]") ~= nil,
      "negative control never reached actual tar unpack: " .. text .. result
    )
    check(not pkg:is_installed(), "corrupt archive left an installed package")
    check(
      vim.fn.filereadable(env("ROOT") .. "/packages/magit-lsp/mason-receipt.json") == 0,
      "failed install has receipt"
    )
    check(vim.fn.executable(env("ROOT") .. "/bin/magit-lsp") == 0, "failed install has executable")
    write({ installed = false, success = success, result = result, output = text })
    return
  end
  check(success == true and pkg:is_installed(), "valid retry failed: " .. text .. tostring(result))
  vim.opt.rtp:append(env("CHECKOUT"))
  require("magit-lsp").setup({ cmd = { env("LAUNCHER") } })
  local client
  check(
    vim.wait(20000, function()
      client = vim.lsp.get_clients({ name = "magit-lsp" })[1]
      if not client or not client.initialized or vim.bo.filetype ~= "magit-lsp" then
        return false
      end
      local status = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
      return status:find("mason-install.txt", 1, true) and status:find("Head:", 1, true)
    end, 20),
    "installed server did not initialize/render status"
  )
  check(client.server_info.version == "0.2.0", "initialize server version mismatch")
  local status = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
  check(status:find("Untracked files", 1, true), "missing untracked section")
  local info = client.server_info
  client:stop()
  check(
    vim.wait(5000, function()
      return #vim.lsp.get_clients() == 0
    end, 20),
    "installed client did not stop"
  )
  write({
    installed = true,
    initialized = true,
    server_info = info,
    status = status,
    clients_stopped = true,
    output = text,
  })
end

vim.api.nvim_create_autocmd("VimEnter", {
  once = true,
  callback = function()
    vim.schedule(function()
      local ok, err = xpcall(main, debug.traceback)
      if not ok then
        io.stderr:write("mason-install: " .. tostring(err) .. "\n")
        vim.cmd("cquit 1")
      else
        vim.cmd("qa!")
      end
    end)
  end,
})
