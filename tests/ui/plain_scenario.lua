-- Test-only observations and standard code-action selection; no project adapter.
local applied = 0
_G.plain_viewport_regression_error = "pending"
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
  -- A server prompt transition rewrites the document while it may be shown in a
  -- window that is not current (a prompt float or another split). Record every
  -- affected window's own view before applying and restore it afterwards, so
  -- the repair is independent of edit shape, timing and the current window.
  local views = {}
  if buf and vim.api.nvim_buf_is_valid(buf) then
    for _, win in ipairs(vim.fn.win_findbuf(buf)) do
      if vim.api.nvim_win_is_valid(win) then
        views[win] = vim.api.nvim_win_call(win, vim.fn.winsaveview)
      end
    end
  end
  local result = apply(...)
  log("applyEdit-after", result)
  if result and result.applied then
    applied = applied + 1
  end
  if result and result.applied and buf and vim.api.nvim_buf_is_valid(buf) then
    local last = vim.api.nvim_buf_line_count(buf)
    for win, view in pairs(views) do
      if vim.api.nvim_win_is_valid(win) then
        local height = vim.api.nvim_win_get_height(win)
        vim.api.nvim_win_call(win, function()
          vim.fn.winrestview({
            lnum = math.min(math.max(view.lnum, 1), last),
            col = view.col,
            coladd = view.coladd,
            curswant = view.curswant,
            topline = math.min(math.max(view.topline, 1), math.max(1, last - height + 1)),
            topfill = 0,
            leftcol = view.leftcol,
            skipcol = view.skipcol,
          })
        end)
      end
    end
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
    viewport_regression = _G.plain_viewport_regression == true,
    viewport_regression_error = _G.plain_viewport_regression_error or "",
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

--- Deterministically regress per-window viewport restoration through the real
--- `workspace/applyEdit` handler: a `.magit` buffer shown in a window that is
--- not current shrinks from a later range, so its stored view must be clamped to
--- the surviving lines and window height. Sets `_G.plain_viewport_regression`
--- and returns whether the affected window's view was restored.
function PlainUI.viewport_regression()
  local origin = vim.api.nvim_get_current_win()
  local origin_view = vim.fn.winsaveview()
  local client = assert(vim.lsp.get_client_by_id(vim.g.plain_smoke_client_id or -1))
  local directory = vim.fn.tempname()
  vim.fn.mkdir(directory, "p")
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_name(buf, directory .. "/.magit")
  local win
  local completed, restored = xpcall(function()
    assert(
      vim.uri_to_bufnr(vim.uri_from_bufnr(buf)) == buf,
      "scratch buffer is not addressable by URI"
    )
    local source = {}
    for i = 1, 30 do
      source[i] = "source row " .. i
    end
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, source)
    vim.cmd("belowright split")
    win = vim.api.nvim_get_current_win()
    vim.api.nvim_win_set_buf(win, buf)
    vim.api.nvim_win_call(win, function()
      vim.fn.winrestview({ lnum = 25, col = 0, topline = 20, topfill = 0 })
    end)
    -- Leave the affected buffer in a non-current window and shrink only a later
    -- range, the shape that the old current-window, offset-zero repair missed.
    vim.api.nvim_set_current_win(origin)
    local edits = {
      {
        range = {
          start = { line = 20, character = 0 },
          ["end"] = { line = 29, character = #source[30] },
        },
        newText = "",
      },
    }
    local result = vim.lsp.handlers["workspace/applyEdit"](nil, {
      edit = {
        documentChanges = {
          {
            textDocument = {
              uri = vim.uri_from_bufnr(buf),
              version = vim.lsp.util.buf_versions[buf] or 0,
            },
            edits = edits,
          },
        },
      },
    }, { client_id = client.id, offset_encoding = client.offset_encoding })
    local top = vim.api.nvim_win_call(win, function()
      return vim.fn.line("w0")
    end)
    local cursor = vim.api.nvim_win_get_cursor(win)
    local count = vim.api.nvim_buf_line_count(buf)
    local height = vim.api.nvim_win_get_height(win)
    return {
      count = count,
      top = top,
      line = cursor[1],
      expected_top = math.max(1, count - height + 1),
      applied = result and result.applied,
      failure = result and result.failureReason,
    }
  end, debug.traceback)
  if win and vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_win_close(win, true)
  end
  if buf and vim.api.nvim_buf_is_valid(buf) then
    vim.api.nvim_buf_delete(buf, { force = true })
  end
  vim.fn.delete(directory, "rf")
  if vim.api.nvim_win_is_valid(origin) then
    vim.api.nvim_set_current_win(origin)
    vim.fn.winrestview(origin_view)
  end
  local ok = completed
    and restored.count < 30
    and restored.top == restored.expected_top
    and restored.line == restored.count
  _G.plain_viewport_regression = ok == true
  _G.plain_viewport_regression_error = completed
      and (ok and "" or ("count=%s top=%s expected=%s line=%s applied=%s failure=%s"):format(
        restored.count,
        restored.top,
        restored.expected_top,
        restored.line,
        tostring(restored.applied),
        tostring(restored.failure)
      ))
    or tostring(restored)
  return _G.plain_viewport_regression
end
