--- Debug API for the UI harness (§A.2–§A.4).
---
--- The module reports only state the client already holds: the views it opened,
--- the render versions it applied and acknowledged, the keymap the server sent,
--- and the requests, prompts and scheduled callbacks the client started itself.
--- It never asks the server for anything, so a snapshot cannot change the UI it
--- describes.

local M = {}

local uv = vim.uv or vim.loop

--- Client work that is still outstanding, counted by the client itself.
M.state = {
  requests = 0,
  prompts = 0,
  scheduled = 0,
  --- Requests queued locally and not answered yet.
  queued = 0,
  --- Progress tokens the server has begun and not ended, by token.
  progress = {},
}

--- Registered views by id.
M.views = {}

--- View ids in stack order, oldest first.
M.order = {}

--- The last 20 notifications shown to the user.
M.notifications = {}

--- Whether the last `idle()` call found the client busy; the trace only emits
--- an `idle` event when that changes to true, so polling does not spam it.
local was_idle = false

local trace_path = vim.env.MAGIT_LSP_TRACE
local trace_file
local trace_started = uv.hrtime()
--- The wall-clock time of `trace_started`, so server events can be merged.
local trace_epoch
do
  local seconds, microseconds = uv.gettimeofday()
  -- Wall clock at `trace_started`; the server merges by this absolute field.
  trace_epoch = seconds * 1000 + microseconds / 1000
end
--- The id of the key press the following events belong to.
local trace_press = 0

if trace_path and trace_path ~= '' then
  trace_file = io.open(vim.fn.expand(trace_path), 'a')
end

--- Append one trace event when `MAGIT_LSP_TRACE` names a file.
---@param event string
---@param detail table|nil
function M.trace(event, detail)
  if not trace_file then
    return
  end
  local elapsed_ms = (uv.hrtime() - trace_started) / 1e6
  local ok, line = pcall(vim.json.encode, {
    t_ms = tonumber(string.format('%.3f', elapsed_ms)),
    t_unix_ms = tonumber(string.format('%.3f', trace_epoch + elapsed_ms)),
    id = trace_press,
    event = event,
    detail = detail or vim.NIL,
  })
  if not ok then
    return
  end
  trace_file:write(line .. '\n')
  trace_file:flush()
end

--- Keep the last 20 notifications the client displayed.
---@param msg any
function M.record_notification(msg)
  M.notifications[#M.notifications + 1] = tostring(msg)
  if #M.notifications > 20 then
    table.remove(M.notifications, 1)
  end
end

--- Called once by the main client before it opens any view.
---@param opts table|nil
function M.configure(opts)
  opts = opts or {}
  if M.notify_wrapped then
    return
  end
  M.notify_wrapped = true
  -- A server progress token keeps `idle` false until its `end` arrives, so a
  -- detached commit cannot be observed half-finished.
  vim.api.nvim_create_autocmd('LspProgress', {
    callback = function(args)
      local params = args.data and args.data.params
      local value = params and params.value
      if type(value) ~= 'table' then
        return
      end
      if value.kind == 'begin' then
        M.progress_started(params.token)
      elseif value.kind == 'end' then
        M.progress_finished(params.token)
      end
    end,
  })
  local original = vim.notify
  vim.notify = function(msg, level, notify_opts)
    M.record_notification(msg)
    return original(msg, level, notify_opts)
  end
end

--- Record a view the client just opened.
---@param id string
---@param kind string
---@param buf integer|nil the view's buffer, used to find the active view
function M.view_open(id, kind, buf)
  if not id then
    return
  end
  local view = M.views[id]
  if not view then
    view = { id = id }
    M.views[id] = view
    M.order[#M.order + 1] = id
  end
  view.kind = kind or view.kind or ''
  view.buf = buf or view.buf
  M.trace('window_opened', { view = id, kind = view.kind, buffer = buf })
end

--- Record that a view is gone.
---@param id string
function M.view_close(id)
  if not M.views[id] then
    return
  end
  M.views[id] = nil
  for index, value in ipairs(M.order) do
    if value == id then
      table.remove(M.order, index)
      break
    end
  end
end

--- Record a render notification as it arrives.
---@param id string
---@param version integer
function M.render_received(id, version)
  M.trace('render_received', { view = id, version = version })
end

--- Record a render the client actually applied to a buffer.
---@param id string
---@param version integer
function M.render_applied(id, version)
  local view = M.views[id]
  if view then
    view.version = version
  end
  M.trace('render_applied', { view = id, version = version })
end

--- Record the version the client told the server it holds.
---@param id string
---@param version integer
function M.render_acked(id, version)
  local view = M.views[id]
  if view then
    view.acked = version
  end
end

--- Record the keymap the server sent, with the keys the client can press.
---@param id string
---@param notice table `view/keymap` params
function M.keymap_received(id, notice)
  local view = M.views[id]
  if not view then
    return
  end
  local keys = {}
  local seen = {}
  local live = notice.liveKeys
  if type(live) == 'table' and #live > 0 then
    for _, chord in ipairs(live) do
      if not seen[chord] then
        seen[chord] = true
        keys[#keys + 1] = chord
      end
    end
  else
    for _, binding in ipairs(notice.bindings or {}) do
      for _, chord in ipairs(binding.keys or {}) do
        if not seen[chord] then
          seen[chord] = true
          keys[#keys + 1] = chord
        end
      end
    end
  end
  view.scope = notice.scope or ''
  view.capture = notice.capture == true
  view.keys = keys
  view.pending = notice.pending or ''
end

--- Record the server's pending menu steps for a view.
---@param id string
---@param pending string|nil
function M.set_pending(id, pending)
  local view = M.views[id]
  if view then
    view.pending = pending or ''
  end
end

--- Start a new trace id for one key press.
---@param steps string[]
function M.key_pressed(steps)
  trace_press = trace_press + 1
  M.trace('key_pressed', { steps = steps })
end

--- The id of the key press being traced, sent with `view/key` (§A.4).
---@return integer
function M.trace_id()
  return trace_press
end

--- Record a server progress token starting; `idle` waits for it to end.
---@param token string|integer
function M.progress_started(token)
  M.state.progress[tostring(token)] = true
end

--- Record a server progress token ending.
---@param token string|integer
function M.progress_finished(token)
  M.state.progress[tostring(token)] = nil
end

--- Record one canonical key step being sent to the server.
---@param step string
---@param id string|nil
function M.key_sent(step, id)
  M.trace('key_sent', { key = step, view = id })
end

--- Count one asynchronous request until `M.request_finished` runs.
function M.request_started()
  M.state.requests = M.state.requests + 1
end

--- Stop counting an asynchronous request.
function M.request_finished()
  M.state.requests = math.max(0, M.state.requests - 1)
end

--- Count one request entering the client's queue.
function M.queued_started()
  M.state.queued = M.state.queued + 1
end

--- Stop counting a queued request once it has been answered or dropped.
function M.queued_finished()
  M.state.queued = math.max(0, M.state.queued - 1)
end

--- Count one prompt the client is waiting on.
function M.prompt_started()
  M.state.prompts = M.state.prompts + 1
end

--- Stop counting a prompt once it is answered or cancelled.
function M.prompt_finished()
  M.state.prompts = math.max(0, M.state.prompts - 1)
end

--- Queue `fn` on Neovim's event loop, counted as outstanding client work.
---@param fn fun()
function M.schedule(fn)
  M.state.scheduled = M.state.scheduled + 1
  vim.schedule(function()
    local ok, err = pcall(fn)
    M.state.scheduled = M.state.scheduled - 1
    if not ok then
      error(err, 0)
    end
  end)
end

--- True when no client work is outstanding and every view is acknowledged.
---@return boolean
function M.idle()
  local busy = M.state.requests > 0
    or M.state.queued > 0
    or M.state.prompts > 0
    or M.state.scheduled > 0
    or next(M.state.progress) ~= nil
  if not busy then
    for _, id in ipairs(M.order) do
      local view = M.views[id]
      if view.version == nil or view.version ~= view.acked then
        busy = true
        break
      end
    end
  end
  if busy then
    was_idle = false
    return false
  end
  if not was_idle then
    was_idle = true
    M.trace('idle', { views = #M.order })
  end
  return true
end

--- The view owning the current buffer, else the top of the stack.
---@return table|nil
local function active_view()
  local buf = vim.api.nvim_get_current_buf()
  for _, id in ipairs(M.order) do
    if M.views[id].buf == buf then
      return M.views[id]
    end
  end
  local id = M.order[#M.order]
  return id and M.views[id] or nil
end

--- One window as the snapshot reports it.
---@param win integer
local function window_state(win)
  local buf = vim.api.nvim_win_get_buf(win)
  local config = vim.api.nvim_win_get_config(win)
  local float = config.relative ~= nil and config.relative ~= ''
  local row, col
  if float then
    row = config.row or 0
    col = config.col or 0
  else
    local position = vim.fn.win_screenpos(win)
    row = math.max(0, position[1] - 1)
    col = math.max(0, position[2] - 1)
  end
  local width = vim.api.nvim_win_get_width(win)
  return {
    id = win,
    float = float,
    row = row,
    col = col,
    width = width,
    height = vim.api.nvim_win_get_height(win),
    -- True when this window spans the whole editor, as the status view does.
    full_width = width == vim.o.columns,
    buffer = buf,
    filetype = vim.bo[buf].filetype,
  }
end

--- The last 20 lines of `:messages`.
local function recent_messages()
  local ok, output = pcall(vim.fn.execute, 'messages')
  if not ok or type(output) ~= 'string' then
    return {}
  end
  local lines = {}
  for line in output:gmatch('[^\n]+') do
    lines[#lines + 1] = line
  end
  local recent = {}
  for index = math.max(1, #lines - 19), #lines do
    recent[#recent + 1] = lines[index]
  end
  return recent
end

--- The §A.2 state snapshot: JSON-encodable and read-only.
---@return table
function M.snapshot()
  local view = active_view()
  local cursor = vim.api.nvim_win_get_cursor(0)
  local windows = {}
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    windows[#windows + 1] = window_state(win)
  end
  local stack = {}
  for _, id in ipairs(M.order) do
    stack[#stack + 1] = { id = id, kind = M.views[id].kind }
  end
  return {
    mode = vim.fn.mode(),
    cursor = { row = cursor[1] - 1, col = cursor[2] },
    idle = M.idle(),
    windows = windows,
    view_stack = stack,
    pending = (view and view.pending) or '',
    render = {
      version = (view and view.version) or 0,
      acked = (view and view.acked) or 0,
    },
    keymap = {
      scope = (view and view.scope) or '',
      capture = (view and view.capture) or false,
      keys = (view and view.keys) or {},
    },
    messages = recent_messages(),
    notifications = vim.deepcopy(M.notifications),
  }
end

return M
