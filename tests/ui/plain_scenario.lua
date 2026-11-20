-- Test-only observations and standard code-action selection; no project adapter.
local applied = 0
local apply = vim.lsp.handlers["workspace/applyEdit"]
vim.lsp.handlers["workspace/applyEdit"] = function(...)
  local params = select(2, ...)
  local changes = params and params.edit and params.edit.documentChanges or {}
  local buf, uri, incoming_version
  for _, change in ipairs(changes) do
    local document = change.textDocument
    if document and (document.uri:match("/%.magit$") or document.uri:match("/COMMIT_EDITMSG$")) then
      uri, incoming_version = document.uri, document.version
      buf = vim.uri_to_bufnr(uri)
      break
    end
  end
  local function log(event, result)
    if not buf or not vim.env.MAGIT_LSP_UI_PROBE_FILE then
      return
    end
    local file = assert(io.open(vim.env.MAGIT_LSP_UI_PROBE_FILE, "a"))
    _G.plain_probe_sequence = (_G.plain_probe_sequence or 0) + 1
    local cursor = buf and vim.api.nvim_buf_is_valid(buf) and vim.api.nvim_win_get_cursor(0) or nil
    local document_changes = params and params.edit and params.edit.documentChanges or {}
    local edits = document_changes[1] and document_changes[1].edits or {}
    file:write(
      vim.json.encode({
        sequence = _G.plain_probe_sequence,
        time_ns = vim.uv.hrtime(),
        event = event,
        uri = uri,
        incoming_version = incoming_version,
        current_version = vim.lsp.util.buf_versions[buf],
        result = result,
        traceback = debug.traceback(),
        mode = vim.api.nvim_get_mode(),
        cmdtype = vim.fn.getcmdtype(),
        topline = vim.fn.line("w0"),
        winline = vim.fn.winline(),
        win_height = vim.api.nvim_win_get_height(0),
        edit_count = #edits,
        first_range = edits[1] and edits[1].range or nil,
        cursor = cursor and { row = cursor[1] - 1, col = cursor[2] } or nil,
        lines = buf and vim.api.nvim_buf_line_count(buf) or nil,
      }),
      "\n"
    )
    file:close()
  end
  log("applyEdit-before")
  local result = apply(...)
  log("applyEdit-after", result)
  if result and result.applied then
    applied = applied + 1
  end
  -- Retain the presentation text this server edit applied so scenarios can poll
  -- for a repair without inferring convergence from document content.
  if
    result
    and result.applied
    and buf
    and uri:match("/%.magit$")
    and vim.api.nvim_buf_is_valid(buf)
  then
    _G.plain_authoritative_text = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), "\n")
  end
  return result
end
for _, method in ipairs({ "view/render", "view/keymap", "view/effects", "view/prompt", "view/pick" }) do
  vim.lsp.handlers[method] = function()
    vim.g.plain_smoke_rich_notifications = vim.g.plain_smoke_rich_notifications + 1
  end
end

--- Test-only standard-LSP observations used by the shared scenario driver.
_G.PlainUI = {}
local PlainUI = _G.PlainUI

--- Return whether the Plain client has initialized and drained its requests.
function PlainUI.idle()
  local client = vim.lsp.get_client_by_id(vim.g.plain_smoke_client_id or -1)
  return client ~= nil and client.initialized and next(client.requests) == nil
end

--- Observe native buffer/client state without synthesizing server structure.
function PlainUI.snapshot()
  local buf = vim.api.nvim_get_current_buf()
  local client = vim.lsp.get_client_by_id(vim.g.plain_smoke_client_id or -1)
  local modules = 0
  for name in pairs(package.loaded) do
    if name:match("magit%-lsp") then
      modules = modules + 1
    end
  end
  local cursor = vim.api.nvim_win_get_cursor(0)
  local text = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
  local authoritative = _G.plain_authoritative_text
  return {
    ready = applied > 0 and client ~= nil and client.initialized,
    idle = PlainUI.idle(),
    uri = vim.uri_from_bufnr(0),
    text = text,
    -- The buffer equals the last applied server text, so no local edit remains.
    authoritative_match = authoritative ~= nil and authoritative == text,
    modified = vim.bo.modified,
    mode = vim.api.nvim_get_mode().mode,
    changedtick = vim.api.nvim_buf_get_changedtick(0),
    document_version = vim.lsp.util.buf_versions[buf],
    draft = vim.api.nvim_buf_get_name(0):match("/COMMIT_EDITMSG$") ~= nil,
    cursor = { row = cursor[1] - 1, col = cursor[2] },
    project_modules = modules,
    rich_notifications = vim.g.plain_smoke_rich_notifications,
    applied_edits = applied,
    messages = vim.split(vim.fn.execute("messages"), "\n"),
    stale_feedback = vim.fn
      .execute("messages")
      :find("This view is stale; please retry on the current view.", 1, true) ~= nil,
    capabilities = client and client.config.capabilities,
  }
end

--- Invoke Neovim's built-in code-action UI with one server command selected.
function PlainUI.action(command, item_key)
  vim.lsp.buf.code_action({
    apply = true,
    filter = function(action)
      local selected = type(action.command) == "table" and action.command or action
      local args = selected.arguments
      local target = args and args[1]
      local matches = type(target) == "table"
        and target.command == command
        and (item_key == nil or (target.target and target.target.itemKey == item_key))
      if matches then
        PlainUI.last_command = vim.deepcopy(selected)
      end
      return matches
    end,
  })
end

--- Replay the last opaque server command to verify visible stale-action refusal.
function PlainUI.replay_last()
  local client = assert(vim.lsp.get_client_by_id(vim.g.plain_smoke_client_id))
  client:exec_cmd(assert(PlainUI.last_command), { bufnr = vim.api.nvim_get_current_buf() })
end
