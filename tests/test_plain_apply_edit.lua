local MiniTest = require("mini.test")
local T = MiniTest.new_set()

local function apply_case(declares_document_changes, stale, two_entries)
  local first = vim.api.nvim_create_buf(true, false)
  local second
  local client_id
  local client
  local request_handler
  local initialize_capabilities
  local opened = 0
  local function cleanup()
    if client then
      client:stop(true)
    end
    for _, buf in ipairs({ first, second }) do
      if buf and vim.api.nvim_buf_is_valid(buf) then
        vim.api.nvim_buf_delete(buf, { force = true })
      end
    end
  end
  local ok, err = xpcall(function()
    vim.api.nvim_buf_set_name(first, vim.fn.tempname())
    vim.api.nvim_buf_set_lines(first, 0, -1, false, { "before" })
    if two_entries then
      second = vim.api.nvim_create_buf(true, false)
      vim.api.nvim_buf_set_name(second, vim.fn.tempname())
      vim.api.nvim_buf_set_lines(second, 0, -1, false, { "second" })
    end
    client_id = vim.lsp.start({
      name = "apply-edit-test",
      capabilities = {
        workspace = { workspaceEdit = { documentChanges = declares_document_changes } },
      },
      cmd = function(dispatchers)
        local rpc = {}
        function rpc.request(method, params, callback)
          if method == "initialize" then
            initialize_capabilities = params.capabilities
            callback(nil, { capabilities = { textDocumentSync = 1 } })
          end
          return true, 1
        end
        function rpc.notify(method)
          if method == "textDocument/didOpen" then
            opened = opened + 1
          end
        end
        function rpc.is_closing()
          return false
        end
        function rpc.terminate() end
        request_handler = dispatchers.server_request
        return rpc
      end,
    }, { bufnr = first })
    assert(client_id)
    client = assert(vim.lsp.get_client_by_id(client_id))
    local deadline = vim.uv.hrtime() + 2e9
    while not client.initialized and vim.uv.hrtime() < deadline do
      vim.wait(10)
    end
    assert(client.initialized)
    MiniTest.expect.equality(
      initialize_capabilities.workspace.workspaceEdit.documentChanges,
      declares_document_changes
    )
    if two_entries then
      vim.lsp.buf_attach_client(second, client_id)
    end
    deadline = vim.uv.hrtime() + 2e9
    while opened < (two_entries and 2 or 1) and vim.uv.hrtime() < deadline do
      vim.wait(10)
    end
    MiniTest.expect.equality(opened, two_entries and 2 or 1)
    vim.api.nvim_buf_set_lines(first, 0, -1, false, { "temporary" })
    vim.api.nvim_buf_set_lines(first, 0, -1, false, { "before" })
    if two_entries then
      vim.api.nvim_buf_set_lines(second, 0, -1, false, { "temporary" })
      vim.api.nvim_buf_set_lines(second, 0, -1, false, { "second" })
    end
    local first_tick = vim.api.nvim_buf_get_changedtick(first)
    local first_version = vim.lsp.util.buf_versions[first]
    assert(type(first_version) == "number" and first_version > 1)
    MiniTest.expect.equality(first_version, first_tick)
    local first_stale = first_version - 1
    assert(first_stale > 0)
    local second_tick, second_version, second_stale
    if two_entries then
      second_tick = vim.api.nvim_buf_get_changedtick(second)
      second_version = vim.lsp.util.buf_versions[second]
      assert(type(second_version) == "number" and second_version > 1)
      MiniTest.expect.equality(second_version, second_tick)
      second_stale = second_version - 1
      assert(second_stale > 0)
    end
    local function entry(buf, text, version, old)
      return {
        textDocument = { uri = vim.uri_from_bufnr(buf), version = version },
        edits = {
          {
            range = {
              start = { line = 0, character = 0 },
              ["end"] = { line = 0, character = #old },
            },
            newText = text,
          },
        },
      }
    end
    local changes
    local expected_first
    if two_entries then
      changes = {
        entry(first, "first!", first_version, "before"),
        entry(second, "second!", second_stale, "second"),
      }
      expected_first = "first!"
    else
      local version = stale and first_stale or first_version
      changes = { entry(first, "after!", version, "before") }
      expected_first = stale and "before" or "after!"
    end
    local params = { edit = { documentChanges = changes } }
    local result, response_error = request_handler("workspace/applyEdit", params)
    MiniTest.expect.equality(response_error, nil)
    MiniTest.expect.equality(result.applied, true)
    MiniTest.expect.equality(
      table.concat(vim.api.nvim_buf_get_lines(first, 0, -1, false), "\n"),
      expected_first
    )
    MiniTest.expect.equality(
      vim.api.nvim_buf_get_changedtick(first),
      first_tick + (expected_first == "before" and 0 or 1)
    )
    MiniTest.expect.equality(
      vim.lsp.util.buf_versions[first],
      vim.api.nvim_buf_get_changedtick(first)
    )
    if two_entries then
      MiniTest.expect.equality(
        table.concat(vim.api.nvim_buf_get_lines(second, 0, -1, false), "\n"),
        "second!"
      )
      MiniTest.expect.equality(vim.api.nvim_buf_get_changedtick(second), second_tick + 1)
      MiniTest.expect.equality(
        vim.lsp.util.buf_versions[second],
        vim.api.nvim_buf_get_changedtick(second)
      )
    end
  end, debug.traceback)
  cleanup()
  if not ok then
    error(err, 0)
  end
end

for _, declares in ipairs({ true, false }) do
  for _, stale in ipairs({ false, true }) do
    T["documentChanges_" .. tostring(declares) .. "_stale_" .. tostring(stale)] = function()
      apply_case(declares, stale, false)
    end
  end
end

T["later_stale_document_change_is_applied"] = function()
  apply_case(true, false, true)
end

return T
