--- Instrumentation hooks for the Neovim client (R-NV-3).
---
--- Product code calls these functions at each point the UI harness observes.
--- The defaults are inert, so a normal start never loads `magit-lsp.debug`:
--- only the test harness and a `MAGIT_LSP_TRACE` run replace them with the
--- recording implementation. `schedule` is not inert because it performs the
--- render work itself; the recorder merely counts it as outstanding.

local M = {}

--- Install any process-wide hooks before the first client starts.
function M.configure() end

--- Record that a prompt started.
function M.prompt_started() end

--- Record that a prompt was answered or cancelled.
function M.prompt_finished() end

--- Record that an asynchronous request started.
function M.request_started() end

--- Record that an asynchronous request finished.
function M.request_finished() end

--- Record that a request entered a client queue.
function M.queued_started() end

--- Record that a queued request was answered or dropped.
function M.queued_finished() end

--- Queue `fn` on Neovim's event loop.
---@param fn fun()
function M.schedule(fn)
  vim.schedule(fn)
end

--- Record a `view/render` notification.
---@param id string
---@param version integer
function M.render_received(id, version)
  local _ = id, version
end

--- Record a render the client applied to a buffer.
---@param id string
---@param version integer
function M.render_applied(id, version)
  local _ = id, version
end

--- Record the version the client acknowledged to the server.
---@param id string
---@param version integer
function M.render_acked(id, version)
  local _ = id, version
end

--- Record the server's pending menu step for a view.
---@param id string
---@param pending string|nil
function M.set_pending(id, pending)
  local _ = id, pending
end

--- Record the keymap the server sent for a view.
---@param id string
---@param notice table `view/keymap` params
function M.keymap_received(id, notice)
  local _ = id, notice
end

--- Record a view the client opened.
---@param id string
---@param kind string
---@param buf integer|nil
function M.view_open(id, kind, buf)
  local _ = id, kind, buf
end

--- Record that a view is gone.
---@param id string
function M.view_close(id)
  local _ = id
end

--- Start a new trace id for one key press.
---@param steps string[]
function M.key_pressed(steps)
  local _ = steps
end

--- The trace id sent with the keys of one press; opaque to the server.
---@return integer
function M.trace_id()
  return 0
end

--- Record one canonical key step being sent to the server.
---@param step string
---@param id string|nil
function M.key_sent(step, id)
  local _ = step, id
end

return M
