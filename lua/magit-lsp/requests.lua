--- The client's asynchronous request layer (R-NV-1).
---
--- Every key, invoke and open is sent with `client:request` through one FIFO
--- queue per LSP client. Requests run strictly one at a time in the order the
--- user produced them, so a key pressed while another request is in flight is
--- queued instead of blocking Neovim or racing the answer. Nothing here waits:
--- a response callback runs on Neovim's event loop, so the editor stays
--- responsive while a slow Git command, prompt or editor bridge is outstanding.

local debug_api = require('magit-lsp.debug')

local uv = vim.uv or vim.loop

local M = {}

--- Milliseconds between checks while a client finishes initializing.
local INIT_POLL_MS = 5
--- Maximum milliseconds to wait for a client to initialize before failing.
local INIT_TIMEOUT_MS = 10000

--- One queue per LSP client id.
---@type table<integer, { running: boolean, items: table[], waiting: boolean|nil, deadline: integer|nil }>
M.queues = {}

--- Error text from an LSP error object.
---@param err any
---@return string
function M.error_message(err)
  if type(err) == 'table' then
    return tostring(err.message or err)
  end
  return tostring(err)
end

--- Called when a client's queue drains, so held acknowledgements can flush.
---@type fun(client_id: integer)|nil
M.on_quiet = nil

--- The queue of `client_id`, creating it on first use.
---@param client_id integer
---@return table
local function queue(client_id)
  local state = M.queues[client_id]
  if not state then
    state = { running = false, items = {} }
    M.queues[client_id] = state
  end
  return state
end

--- Run the next request of `client_id`, if the queue is free.
---
--- A request is never sent before the client's `initialize` handshake has
--- completed: Neovim drops such a request, so while the client is not ready
--- the queue waits on a short timer instead of blocking the event loop.
---@param client_id integer
function M.pump(client_id)
  local state = M.queues[client_id]
  if not state or state.running then
    return
  end
  if #state.items == 0 then
    M.queues[client_id] = nil
    return
  end

  local c = vim.lsp.get_client_by_id(client_id)
  if not c then
    M.finish(client_id, table.remove(state.items, 1), nil, { message = 'the LSP client is not running' })
    return
  end

  if not c.initialized then
    local deadline = state.deadline or (uv.hrtime() + INIT_TIMEOUT_MS * 1e6)
    state.deadline = deadline
    if uv.hrtime() >= deadline then
      state.waiting = nil
      state.deadline = nil
      M.finish(client_id, table.remove(state.items, 1), nil, { message = 'the LSP client did not initialize' })
      return
    end
    if state.waiting then
      return
    end
    state.waiting = true
    vim.defer_fn(function()
      if M.queues[client_id] ~= state then
        return
      end
      state.waiting = nil
      M.pump(client_id)
    end, INIT_POLL_MS)
    return
  end
  state.waiting = nil
  state.deadline = nil

  local item = table.remove(state.items, 1)
  state.running = true

  debug_api.request_started()
  local finished = false

  --- Release the queue slot after the response's follow-up notifications.
  local function release()
    debug_api.queued_finished()
    if M.queues[client_id] ~= state then
      return
    end
    state.running = false
    if #state.items == 0 and M.on_quiet then
      M.on_quiet(client_id)
    end
    M.pump(client_id)
  end

  --- Finish this request once, then release the queue after one loop turn.
  ---
  --- The server sends a response before the render and keymap it produced, so
  --- the client must let those notifications arrive and be applied (and the
  --- render acknowledged) before the next queued request leaves. Sending the
  --- next key first would use a stale version, and its acknowledgement could
  --- block the server's reader while a key handler waits on a prompt.
  ---@param err any
  ---@param result any
  local function handler(err, result)
    if finished then
      return
    end
    finished = true
    debug_api.request_finished()
    local ok, failure = pcall(item.on_result, result, err)
    vim.defer_fn(function()
      release()
      if not ok then
        error(failure, 0)
      end
    end, 0)
  end

  if item.on_send then
    item.on_send()
  end
  local ok, sent = pcall(c.request, c, item.method, item.params, handler, item.buf)
  if not ok then
    handler(nil, { message = tostring(sent) })
  elseif sent == false then
    handler(nil, { message = 'the client did not accept ' .. item.method })
  end
end

--- Finish one request and call its handler exactly once.
---@param client_id integer
---@param item table
---@param result any
---@param err any
function M.finish(client_id, item, result, err)
  local state = M.queues[client_id]
  if state then
    state.running = false
  end
  debug_api.queued_finished()
  local ok, failure = pcall(item.on_result, result, err)
  if state and M.queues[client_id] == state and #state.items == 0 and M.on_quiet then
    M.on_quiet(client_id)
  end
  M.pump(client_id)
  if not ok then
    error(failure, 0)
  end
end

--- Queue one request for `client_id`.
---
--- `spec` holds `method`, `params`, `buf` and `on_result(result, err)`.
--- `on_result` runs in queue order; `spec.on_send` (optional) runs just before
--- the request leaves the client, for tracing.
---@param client_id integer
---@param spec table { method: string, params: table, buf: integer, on_result: fun(result: any, err: any), on_send: fun()|nil }
function M.enqueue(client_id, spec)
  local state = queue(client_id)
  state.items[#state.items + 1] = spec
  debug_api.queued_started()
  M.pump(client_id)
end

--- Number of requests waiting or running for `client_id`.
---@param client_id integer
---@return integer
function M.pending(client_id)
  local state = M.queues[client_id]
  if not state then
    return 0
  end
  return #state.items + (state.running and 1 or 0)
end

--- Drop every queued request of a stopped client without running its handler.
---@param client_id integer
function M.clear(client_id)
  local state = M.queues[client_id]
  if state then
    for _ = 1, #state.items do
      debug_api.queued_finished()
    end
  end
  M.queues[client_id] = nil
end

return M
