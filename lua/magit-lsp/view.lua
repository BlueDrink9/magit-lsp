--- The single Neovim view manager (R-NV-2).
---
--- Every server view — status, log, process log, menu — is one record keyed by
--- its client and the server's `viewId`. Placement decides
--- only where its buffer is shown; render, keymap, key, invoke, resync, ack and
--- buffering are kind-independent. Editors that show a real file (the editor
--- bridge) register their own placement and reuse the same lifetime code.

local buffer = require('magit-lsp.buffer')
local keymap = require('magit-lsp.keymap')
local debug_api = require('magit-lsp.debug')
local requests = require('magit-lsp.requests')

local M = {}

--- Open views by composite client/view key. Real-file documents share the table.
---@type table<string, table>
M.views = {}

--- Server views in opening order, oldest first. Documents are excluded so the
--- debug stack and focus-fallback only describe server-owned views.
---@type string[]
M.order = {}

--- Renders and keymaps that arrived before their descriptor, by view id.
---@type table<string, table[]>
M.pending_render = {}
M.pending_keymap = {}

--- Renders held while a resync is outstanding, by view key.
---@type table<string, table[]>
M.deferred_render = {}

--- Placement implementations, keyed by descriptor placement.
---@type table<string, table>
M.placements = {}

--- Callbacks into the main client.
M.host = {}

--- Called by `init.lua` once the LSP client and config exist.
---@param opts table presentation and lifecycle callbacks
function M.configure(opts)
  for key, value in pairs(opts or {}) do
    M.host[key] = value
  end
  M.placements.main = M.placements.main or { place = M.place_main }
  M.placements.side = M.placements.side or { place = M.place_side }
  M.placements.float = M.placements.float or { place = M.place_float, fit = M.fit_float }
end

--- Register one descriptor placement.
---@param name string `main`, `side`, `bottom`, `float` or a document placement
---@param spec table { place: fun(view, opts): integer, fit: fun(view)|nil }
function M.register_placement(name, spec)
  M.placements[name] = spec
end

local function client(client_id)
  if not client_id then
    return nil
  end
  return vim.lsp.get_client_by_id(client_id)
end

local function error_message(err)
  if type(err) == 'table' then
    return tostring(err.message or err)
  end
  return tostring(err)
end

--- The key of a view inside this Neovim instance. Server view ids are only
--- unique per LSP client, so two repositories can both have `v1`.
---@param client_id integer|nil
---@param view_id string
---@return string
local function view_key(client_id, view_id)
  return tostring(client_id or 0) .. '\0' .. tostring(view_id)
end

local flush_deferred

local function find(id, client_id)
  if not id then
    return nil
  end
  if client_id then
    return M.views[view_key(client_id, id)]
  end
  return M.views[id]
end

--- Resolve a local key, or a server id scoped to one client.
---@param id string local key or server id when client_id is supplied
---@param client_id integer|nil
---@return table|nil
function M.find(id, client_id)
  return find(id, client_id)
end

local function view_for_buf(buf)
  for _, record in pairs(M.views) do
    if record and record.buf == buf then
      return record
    end
  end
  return nil
end

local function remove_from_order(key)
  for index, value in ipairs(M.order) do
    if value == key then
      table.remove(M.order, index)
      break
    end
  end
end

--- The open view whose buffer is current, else the newest server view.
---@return table|nil
function M.current()
  local current = view_for_buf(vim.api.nvim_get_current_buf())
  if current then
    return current
  end
  local id = M.order[#M.order]
  return id and M.views[id] or nil
end

--- The first status view, optionally for one repository.
---@param root string|nil
---@return table|nil
function M.status(root)
  for _, id in ipairs(M.order) do
    local view = M.views[id]
    if view and view.kind == 'status' and (root == nil or view.root == root) then
      return view
    end
  end
  return nil
end

--- The newest view belonging to `client_id`.
---@param client_id integer|nil
---@return table|nil
function M.last_for_client(client_id)
  for index = #M.order, 1, -1 do
    local view = M.views[M.order[index]]
    if view and view.client_id == client_id then
      return view
    end
  end
  return nil
end

--- Focus a view's window, if it still has one.
---@param id string local key or server view id
---@param client_id integer|nil
---@return boolean focused
function M.focus(id, client_id)
  local view = find(id, client_id)
  if not view then
    return false
  end
  local win = view.win
  if not win or not vim.api.nvim_win_is_valid(win) or vim.api.nvim_win_get_buf(win) ~= view.buf then
    win = vim.fn.bufwinid(view.buf)
  end
  if win ~= -1 and vim.api.nvim_win_is_valid(win) then
    view.win = win
    vim.api.nvim_set_current_win(win)
    return true
  end
  return false
end

--- Focus the base status view of `view`'s session.
---@param view table|nil
function M.focus_base(view)
  local base = M.status(view and view.root)
  if base then
    M.focus(base.key)
  elseif view then
    local previous = M.last_for_client(view.client_id)
    if previous then
      M.focus(previous.key)
    end
  end
end

local function focus_previous(view)
  for index = #M.order, 1, -1 do
    local id = M.order[index]
    if id ~= (view and view.key) and M.focus(id) then
      return
    end
  end
  if view then
    M.focus_base(view)
  end
end

--- Resolve the server uri of a view, for positional requests.
---@param view table|nil
---@return string
function M.uri(view)
  view = view or M.current()
  if view and view.uri then
    return view.uri
  end
  return view and vim.uri_from_bufnr(view.buf) or ''
end

--- Queue one asynchronous LSP request for `buf` and call `on_result`.
---
--- Requests for one client run one at a time in enqueue order, so a key, an
--- invoke or an open never blocks Neovim's event loop, and a request made
--- while another is in flight waits its turn instead of racing it.
---@param buf integer
---@param client_id integer|nil
---@param method string
---@param params table|fun(): table
---@param on_result fun(result: any, err: any)|nil
function M.request(buf, client_id, method, params, on_result)
  requests.enqueue(client_id, {
    method = method,
    buf = buf,
    params = params,
    on_result = on_result or function() end,
  })
end

--- Acknowledgements held while a request is in flight, keyed by view key.
---@type table<string, { client_id: integer, view_id: string, version: integer }>
M.pending_ack = {}

--- Send, or hold, the acknowledgement of `view`'s applied version.
---
--- A `view/rendered` notification is handled inline by the server's reader,
--- where it needs the session lock that a key handler can hold while a prompt
--- waits. Acknowledging while a request is in flight could therefore block the
--- prompt answer behind the ack, so it is held until the queue drains.
---@param view table
local function ack(view)
  if requests.pending(view.client_id) > 0 then
    M.pending_ack[view.key] = {
      client_id = view.client_id,
      view_id = view.id,
      version = view.version,
    }
    return
  end
  local c = client(view.client_id)
  if c then
    c:notify('view/rendered', { viewId = view.id, version = view.version })
    debug_api.render_acked(view.key, view.version)
  end
end

--- Send the acknowledgements held for `client_id`, dropping closed views.
---@param client_id integer
function M.flush_acks(client_id)
  local keys = {}
  for key, pending in pairs(M.pending_ack) do
    if pending.client_id == client_id then
      keys[#keys + 1] = key
    end
  end
  table.sort(keys)
  for _, key in ipairs(keys) do
    local pending = M.pending_ack[key]
    M.pending_ack[key] = nil
    local view = M.views[key]
    local c = client(pending.client_id)
    if view and c then
      c:notify('view/rendered', { viewId = pending.view_id, version = pending.version })
      debug_api.render_acked(key, pending.version)
    end
  end
end

requests.on_quiet = M.flush_acks

--- Ask Neovim to re-request semantic tokens for `buf`.
--- The server owns highlighting, but only sends tokens on request.
---@param buf integer
local function refresh_tokens(buf)
  debug_api.schedule(function()
    if buf and vim.api.nvim_buf_is_valid(buf) then
      vim.lsp.semantic_tokens.force_refresh(buf)
    end
  end)
end

local function set_cursor(view, cursor)
  local win = view.win
  if not win or not vim.api.nvim_win_is_valid(win) then
    win = vim.fn.bufwinid(view.buf)
  end
  if win == -1 then
    return
  end
  local line = math.min(cursor.line, vim.api.nvim_buf_line_count(view.buf) - 1)
  local text = vim.api.nvim_buf_get_lines(view.buf, line, line + 1, false)[1] or ''
  local column = math.min(cursor.column, #text)
  vim.api.nvim_win_set_cursor(win, { line + 1, column })
end


--- Hold renders of a view while its full resync is outstanding.
---@param view table
local function request_resync(view)
  local c = client(view.client_id)
  if not c or view.resyncing then
    return
  end
  view.resyncing = true
  debug_api.request_started()
  c:request('view/resync', { viewId = view.id }, function(err, result)
    debug_api.request_finished()
    view.resyncing = false
    if err or type(result) ~= 'table' then
      flush_deferred(view.key)
      return
    end
    M.apply_render(result, view.client_id)
    flush_deferred(view.key)
  end, view.buf)
end

--- Apply a `view/render` payload, then acknowledge the version.
---
--- A client that does not hold the edit's base version has lost an update and
--- asks for a full resync; renders that arrive while that resync is
--- outstanding are held and applied after it. A render that arrives before the
--- descriptor is buffered keyed by its client and server view id, never
--- dropped.
---@param params table
---@param client_id integer the client that sent the notification
function M.apply_render(params, client_id)
  if not params or not params.viewId or not client_id then
    return
  end
  local key = view_key(client_id, params.viewId)
  local view = M.views[key]
  debug_api.render_received(key, params.version)
  debug_api.set_pending(key, params.pending)
  if not view then
    local queue = M.pending_render[key] or {}
    queue[#queue + 1] = { client_id = client_id, params = params }
    M.pending_render[key] = queue
    return
  end
  if view.document then
    return
  end

  -- A resync replaces the whole text; applying later edits on top of a
  -- version the client does not hold yet would corrupt the buffer.
  if view.resyncing then
    local queue = M.deferred_render[key] or {}
    queue[#queue + 1] = params
    M.deferred_render[key] = queue
    return
  end

  if not params.text and params.edits and params.baseVersion ~= view.version then
    request_resync(view)
    return
  end

  -- Leaving text mode: drop undo history before replacing the text, so the
  -- server's version cannot be undone back into the user's edits.
  if params.mode == 'text' and view.mode ~= 'text' then
    view.mode = 'text'
    buffer.enter_text_mode(view.buf)
  elseif params.mode == 'interactive' and view.mode == 'text' then
    view.mode = 'interactive'
    buffer.exit_text_mode(view.buf)
  end

  local text_mode = view.mode == 'text'
  if params.text then
    buffer.set_lines(view.buf, 0, -1, params.text, text_mode)
  elseif params.edits then
    buffer.apply_edits(view.buf, params.edits, text_mode)
  end

  if params.mode then
    view.mode = params.mode
  end
  view.lines = vim.api.nvim_buf_get_lines(view.buf, 0, -1, false)
  view.version = params.version
  debug_api.render_applied(key, params.version)
  if params.cursor then
    set_cursor(view, params.cursor)
  end
  -- Ack before asking for tokens: positional requests resolve against the
  -- acknowledged tree, so the new version must be recorded first.
  ack(view)
  local placement = M.placements[view.placement]
  if placement and placement.fit then
    placement.fit(view)
  end
  refresh_tokens(view.buf)
end

--- Apply renders held while `key`'s resync was outstanding.
---@param key string|nil view key; nil flushes every held render
function flush_deferred(key)
  if key then
    local queue = M.deferred_render[key]
    if not queue then
      return
    end
    M.deferred_render[key] = nil
    local view = M.views[key]
    for _, params in ipairs(queue) do
      if view and M.views[key] == view and not view.resyncing then
        M.apply_render(params, view.client_id)
      end
    end
    return
  end
  local keys = {}
  for queued in pairs(M.deferred_render) do
    keys[#keys + 1] = queued
  end
  for _, queued in ipairs(keys) do
    flush_deferred(queued)
  end
end

--- Apply a `view/keymap` notification atomically (§3.9).
---
--- The map set is the server's bindings translated to Neovim keys plus the
--- client-side fold mirrors and, for the status scope, the temporary visit
--- exception until `common.visit` is implemented.
---@param notice table `view/keymap` params
---@param client_id integer the client that sent the notification
function M.apply_keymap(notice, client_id)
  if type(notice) ~= 'table' or not notice.viewId or not client_id then
    return
  end
  local key = view_key(client_id, notice.viewId)
  local view = M.views[key]
  if not view then
    local queue = M.pending_keymap[key] or {}
    queue[#queue + 1] = { client_id = client_id, notice = notice }
    M.pending_keymap[key] = queue
    return
  end
  view.keymap = notice

  if notice.mode == 'text' then
    if view.mode ~= 'text' then
      view.mode = 'text'
      buffer.enter_text_mode(view.buf)
    end
  elseif view.mode == 'text' then
    view.mode = 'interactive'
    buffer.exit_text_mode(view.buf)
  end

  local handlers = {
    press = function(steps)
      M.press(steps, view.key)
    end,
    command = function(command, args)
      M.invoke(command, { args = args }, view.key)
    end,
  }

  local opts
  if view.keymap_options then
    handlers, opts = view.keymap_options(view, notice)
  else
    local visit
    if notice.scope == 'status' and notice.mode ~= 'text' then
      local binding
      for _, candidate in ipairs(notice.bindings or {}) do
        if candidate.command == 'common.visit' then
          binding = candidate
          break
        end
      end
      if not binding or binding.status ~= 'implemented' then
        visit = { lhs = view.visit_key or M.host.visit_key or '<CR>', fn = function()
          if M.host.visit then
            M.host.visit(view)
          end
        end }
      end
    end
    opts = { visit = visit, fold_keys = view.fold_keys or M.host.fold_keys }
  end

  keymap.apply(view.buf, notice, handlers, opts)
  debug_api.keymap_received(view.key, notice)
  view.capture = notice.capture == true

  -- Backspace pops one server-side pending step; it is not printable, so the
  -- capture keymap does not carry it.
  if view.capture and not view.bs_mapped then
    view.bs_mapped = true
    vim.keymap.set({ 'n', 'x' }, '<BS>', function()
      M.press({ 'Backspace' }, view.key)
    end, { buffer = view.buf, nowait = true, silent = true, desc = 'Pop a pending step' })
  elseif not view.capture and view.bs_mapped then
    view.bs_mapped = false
    pcall(vim.keymap.del, { 'n', 'x' }, '<BS>', { buffer = view.buf })
  end
  view.keymap_ready = true
end

local function place_main(view, opts)
  local win = opts and opts.win or vim.api.nvim_get_current_win()
  local previous = view_for_buf(vim.api.nvim_win_get_buf(win))
  if previous then
    view.previous = previous.key
  end
  vim.api.nvim_win_set_buf(win, view.buf)
  buffer.enable_folding(win)
  return win
end

local function place_side(view)
  local width = math.max(20, math.min(80, math.floor(vim.o.columns / 2)))
  pcall(vim.cmd, 'botright ' .. width .. 'vsplit')
  local win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, view.buf)
  buffer.enable_folding(win)
  return win
end

local function float_config(view)
  local lines = #(view.lines or {})
  local width = math.max(20, math.min(vim.o.columns - 4, 80))
  local height = math.max(1, math.min(lines, vim.o.lines - 4))
  local row = math.max(0, math.floor((vim.o.lines - height) / 2))
  local col = math.max(0, math.floor((vim.o.columns - width) / 2))
  return {
    relative = 'editor',
    row = row,
    col = col,
    width = width,
    height = height,
    style = 'minimal',
    border = 'single',
  }
end

local function place_float(view)
  return vim.api.nvim_open_win(view.buf, true, float_config(view))
end

--- Re-fit a generic floating descriptor after a render or editor resize.
---@param view table
function M.fit_float(view)
  if view.win and vim.api.nvim_win_is_valid(view.win) then
    vim.api.nvim_win_set_config(view.win, float_config(view))
  end
end

--- Place a main descriptor in the selected window, remembering its predecessor.
M.place_main = place_main
--- Place a side descriptor in a vertical split.
M.place_side = place_side
--- Place a float descriptor in a centered floating window.
M.place_float = place_float

local function attach(view)
  local c = client(view.client_id)
  if c and vim.lsp.buf_attach_client and not vim.lsp.buf_is_attached(view.buf, view.client_id) then
    pcall(vim.lsp.buf_attach_client, view.buf, view.client_id)
  end
end

local function on_wiped(key)
  local view = M.views[key]
  if not view or view.closing then
    return
  end
  M.views[key] = nil
  M.pending_render[key] = nil
  M.pending_keymap[key] = nil
  M.deferred_render[key] = nil
  M.pending_ack[key] = nil
  remove_from_order(key)
  if not view.document then
    debug_api.view_close(key)
  end
  if view.document then
    if view.on_closed then
      view.on_closed(view)
    end
    return
  end
  local c = client(view.client_id)
  if c then
    c:notify('view/close', { viewId = view.id })
  end
  focus_previous(view)
end

--- Open (or focus) the window for a server descriptor.
---@param descriptor table `viewDescriptor` from `view/open` or a push effect
---@param opts table|nil { buf?: integer, win?: integer, client_id?: integer, root?: string, placement?: string } `placement` selects a client window implementation and leaves the descriptor's placement advisory
---@return table|nil view
function M.open(descriptor, opts)
  if not descriptor or not descriptor.id then
    return nil
  end
  opts = opts or {}
  local client_id = opts.client_id
  local key = view_key(client_id, descriptor.id)
  local existing = M.views[key]
  if existing then
    M.focus(key)
    return existing
  end

  local pending = M.pending_render[key]
  local last = pending and pending[#pending]
  local view = {
    id = descriptor.id,
    key = key,
    kind = descriptor.kind,
    uri = descriptor.uri,
    title = descriptor.title,
    placement = descriptor.placement or 'main',
    mode = descriptor.mode or 'interactive',
    scope = descriptor.scope,
    capture = descriptor.capture == true,
    descriptor = descriptor,
    client_id = client_id,
    root = opts.root,
    visit_key = opts.visit_key,
    fold_keys = opts.fold_keys,
    version = nil,
    lines = (last and last.params.text) or {},
  }
  M.views[key] = view

  if opts.buf and vim.api.nvim_buf_is_valid(opts.buf) then
    view.buf = opts.buf
    buffer.rename(view.buf, descriptor.uri)
  else
    local stale = vim.fn.bufnr(descriptor.uri)
    if stale ~= -1 then
      pcall(vim.api.nvim_buf_delete, stale, { force = true })
    end
    view.buf = buffer.fresh(descriptor.uri)
  end

  if view.mode == 'text' then
    buffer.enter_text_mode(view.buf)
  end

  M.order[#M.order + 1] = key

  -- A client may choose the window implementation for a descriptor (the
  -- status view's placement is a client option, R-UI-7); the descriptor's own
  -- placement stays advisory and still decides close/focus behaviour.
  local placement = M.placements[opts.placement or view.placement] or M.placements.main
  view.win = placement.place(view, opts)
  attach(view)
  debug_api.view_open(key, view.kind, view.buf)

  vim.api.nvim_create_autocmd('BufWipeout', {
    buffer = view.buf,
    callback = function()
      on_wiped(key)
    end,
  })

  -- A user can add fold mappings while the server is running; re-scan them
  -- whenever the view regains focus.
  vim.api.nvim_create_autocmd('BufEnter', {
    buffer = view.buf,
    callback = function()
      if view.keymap and M.views[key] then
        M.apply_keymap(view.keymap, view.client_id)
      end
    end,
  })

  -- Notifications can beat the descriptor's effect; apply them in order now
  -- that the window and buffer exist.
  local renders = M.pending_render[key]
  M.pending_render[key] = nil
  for _, entry in ipairs(renders or {}) do
    M.apply_render(entry.params, entry.client_id or client_id)
  end
  local notices = M.pending_keymap[key]
  M.pending_keymap[key] = nil
  for _, entry in ipairs(notices or {}) do
    M.apply_keymap(entry.notice, entry.client_id or client_id)
  end

  M.resize(key)
  flush_deferred(key)
  return view
end

--- Open a real file for an editor-bridge document.
---
--- The document is not a server view: it has no render stream and is excluded
--- from the debug view stack. Placement and key handling come from the caller.
---@param effect table `editDocument` effect
---@param spec table { placement: string, filetype: string, client_id?: integer, root?: string, base?: string, keymap_options: fun(record, notice): table, table, on_closed: fun(record)|nil }
---@return table|nil record
function M.open_document(effect, spec)
  if type(effect) ~= 'table' or not effect.editId then
    return nil
  end
  spec = spec or {}
  local client_id = spec.client_id
  local key = view_key(client_id, effect.editId)
  local existing = M.views[key]
  if existing then
    M.focus(key)
    return existing
  end
  local record = {
    id = effect.editId,
    key = key,
    kind = effect.kind,
    uri = effect.path,
    title = effect.title,
    placement = spec.placement,
    mode = 'interactive',
    document = true,
    client_id = client_id,
    root = spec.root,
    base = spec.base,
    keymap_options = spec.keymap_options,
    on_closed = spec.on_closed,
    lines = {},
  }
  M.views[key] = record
  local placement = M.placements[record.placement]
  if not placement then
    M.views[key] = nil
    return nil
  end
  record.win = placement.place(record)
  if spec.filetype and record.buf then
    vim.bo[record.buf].filetype = spec.filetype
  end
  attach(record)

  vim.api.nvim_create_autocmd({ 'BufWipeout', 'BufDelete' }, {
    buffer = record.buf,
    callback = function()
      on_wiped(key)
    end,
  })

  local pending = M.pending_keymap[key]
  M.pending_keymap[key] = nil
  for _, entry in ipairs(pending or {}) do
    M.apply_keymap(entry.notice, entry.client_id or client_id)
  end
  return record
end

--- Close a view and delete its buffer.
---@param id string local view key or server view id
---@param focus_previous_window boolean|nil
---@param client_id integer|nil used to resolve a bare server view id
function M.close(id, focus_previous_window, client_id)
  local view = find(id, client_id)
  if not view then
    return
  end
  local key = view.key
  view.closing = true
  M.views[key] = nil
  M.pending_render[key] = nil
  M.pending_keymap[key] = nil
  M.deferred_render[key] = nil
  M.pending_ack[key] = nil
  remove_from_order(key)
  if not view.document then
    debug_api.view_close(key)
  end

  local win = view.win
  local restored = false
  if view.placement == 'main' and view.previous then
    local previous = M.views[view.previous]
    if previous and previous.buf and vim.api.nvim_buf_is_valid(previous.buf) then
      if win and vim.api.nvim_win_is_valid(win) then
        pcall(vim.api.nvim_win_set_buf, win, previous.buf)
        previous.win = win
        restored = true
      end
    end
  end
  if not restored and win and vim.api.nvim_win_is_valid(win) then
    if view.placement == 'main' then
      if #vim.api.nvim_list_wins() > 1 then
        pcall(vim.api.nvim_win_close, win, true)
      end
    else
      pcall(vim.api.nvim_win_close, win, true)
    end
  end

  if view.buf and vim.api.nvim_buf_is_valid(view.buf) then
    pcall(vim.api.nvim_buf_delete, view.buf, { force = true })
  end

  if focus_previous_window then
    if restored then
      M.focus(view.previous)
    else
      M.focus_base(view)
    end
  end
end

--- Close this client's views after the server clears a root session.
---@param source table|nil
function M.quit(source)
  local source_view = source or M.current()
  local client_id = source_view and source_view.client_id
  local open = {}
  for _, key in ipairs(M.order) do
    open[#open + 1] = key
  end
  for index = #open, 1, -1 do
    local key = open[index]
    local view = M.views[key]
    if view and not view.document then
      if client_id == nil or view.client_id == client_id then
        M.close(key, false)
      end
    end
  end
end

--- Close every view, e.g. on VimLeavePre.
function M.close_all()
  local open = {}
  for _, view in pairs(M.views) do
    open[#open + 1] = view
  end
  for _, view in ipairs(open) do
    view.closing = true
    M.close(view.key, false)
  end
end

--- The cursor, visual selection and editor mode the server should act on.
---@param view table
---@return table cursor, table|nil selection, string mode
local function position(view)
  local mode = 'normal'
  if vim.api.nvim_get_current_buf() == view.buf then
    local raw = vim.fn.mode()
    if raw == 'v' or raw == 'V' or raw == '\22' then
      mode = 'visual'
    end
  end
  local selection
  if mode == 'visual' then
    local first, last = vim.fn.line('v'), vim.fn.line('.')
    if first ~= 0 and last ~= 0 then
      selection = { startLine = math.min(first, last) - 1, endLine = math.max(first, last) - 1 }
    end
  end
  local win = view.win
  if not win or not vim.api.nvim_win_is_valid(win) or vim.api.nvim_win_get_buf(win) ~= view.buf then
    win = vim.fn.bufwinid(view.buf)
  end
  local cursor = { line = 0, column = 0 }
  if win ~= -1 then
    local p = vim.api.nvim_win_get_cursor(win)
    cursor = { line = p[1] - 1, column = p[2] }
  end
  return cursor, selection, mode
end

--- Send one key chord to a view and apply each answer as it arrives.
---
--- The steps and any key pressed after them queue behind the request in
--- flight, so the server sees them in the order the user produced them and a
--- slow command never blocks the editor. The cursor and selection are read
--- when the key is pressed: they describe what the user was looking at.
---@param steps string[] canonical key steps
---@param id string|nil local view key or server view id; defaults to focused view
function M.press(steps, id)
  local view = id and find(id) or M.current()
  if not view then
    return
  end
  debug_api.key_pressed(steps)
  local cursor, selection, mode = position(view)
  for _, step in ipairs(steps) do
    local key = view.key
    requests.enqueue(view.client_id, {
      method = 'view/key',
      buf = view.buf,
      params = {
        viewId = view.id,
        version = view.version or 0,
        key = step,
        editorMode = mode,
        cursor = cursor,
        selection = selection,
        traceId = debug_api.trace_id(),
      },
      on_send = function()
        debug_api.key_sent(step, key)
      end,
      on_result = function(resp, err)
        if M.views[key] ~= view then
          return
        end
        if not resp then
          vim.notify('magit-lsp key ' .. step .. ' failed: ' .. error_message(err), vim.log.levels.WARN)
          flush_deferred(key)
          return
        end
        M.apply_effects(resp.effects, view)
        flush_deferred(key)
      end,
    })
  end
end

--- Run a command against a view and apply its effects asynchronously.
---
--- The command is queued behind any request in flight; its effects and render
--- are applied when the server answers.
---@param command string
---@param opts table|nil { cursor?, selection?: boolean|table, args?: table }
---@param id string|nil local view key or server view id; defaults to focused view
---@return boolean|nil queued, string|nil err
function M.invoke(command, opts, id)
  opts = opts or {}
  local view = id and find(id) or M.current()
  if not view then
    return nil, 'no view is open'
  end
  local cursor, visual_selection = position(view)
  if opts.cursor then
    cursor = opts.cursor
  end
  local selection = opts.selection
  if selection == true then
    selection = visual_selection
  elseif selection == false then
    selection = nil
  end

  requests.enqueue(view.client_id, {
    method = 'view/invoke',
    buf = view.buf,
    params = {
      viewId = view.id,
      command = command,
      cursor = cursor,
      selection = selection,
      args = opts.args,
      traceId = debug_api.trace_id(),
    },
    on_result = function(resp, err)
      if not resp then
        if M.host.set_error_detail then
          M.host.set_error_detail(type(err) == 'table' and err.data and err.data.detail or nil)
        end
        vim.notify(
          'magit-lsp ' .. command .. ' failed: ' .. error_message(err),
          vim.log.levels.WARN
        )
        flush_deferred(view.key)
        return
      end
      M.apply_effects(resp.effects, view)
      flush_deferred(view.key)
    end,
  })
  return true
end

--- Apply effects produced by a key, command or detached notification.
---@param effects table[]
---@param source table|nil the view the effects came from
function M.apply_effects(effects, source)
  local client_id = source and source.client_id
  for _, effect in ipairs(effects or {}) do
    local kind = effect.type
    if kind == 'message' then
      if M.host.notify then
        M.host.notify(effect)
      end
    elseif kind == 'clipboard' then
      if M.host.clipboard then
        M.host.clipboard(effect.text or '')
      end
    elseif kind == 'passThrough' then
      if M.host.replay then
        M.host.replay(effect.key)
      end
    elseif kind == 'pushView' or kind == 'openView' then
      M.open(effect.descriptor, {
        client_id = client_id,
        root = source and source.root,
      })
    elseif kind == 'popView' then
      M.close(effect.viewId, true, client_id)
    elseif kind == 'focusView' then
      M.focus(effect.viewId, client_id)
    elseif kind == 'quit' then
      M.quit(source)
    elseif kind == 'editDocument' then
      if M.host.edit_document then
        M.host.edit_document(effect, source)
      end
    end
  end
end

--- Send the current viewport of `id` to the server when it changed.
---@param id string local view key or server view id
function M.resize(id)
  local view = find(id)
  if not view or view.document then
    return
  end
  local win = view.win
  if not win or not vim.api.nvim_win_is_valid(win) then
    win = vim.fn.bufwinid(view.buf)
  end
  if win == -1 then
    return
  end
  view.win = win
  local placement = M.placements[view.placement]
  if placement and placement.fit then
    placement.fit(view)
    if not vim.api.nvim_win_is_valid(win) then
      return
    end
  end
  local cols = vim.api.nvim_win_get_width(win)
  local rows = vim.api.nvim_win_get_height(win)
  if view.cols == cols and view.rows == rows then
    return
  end
  view.cols = cols
  view.rows = rows
  local c = client(view.client_id)
  if c then
    c:notify('view/resize', { viewId = view.id, cols = cols, rows = rows })
  end
end

local function collect_symbol_lines(symbols, kind, lines)
  for _, symbol in ipairs(symbols or {}) do
    if symbol.kind == kind then
      lines[#lines + 1] = symbol.range.start.line + 1
    end
    collect_symbol_lines(symbol.children, kind, lines)
  end
end

--- Move within a view using server-owned document symbols, never rendered text.
---@param record table
---@param symbols table[]
---@param kind string section, file or hunk
---@param dir integer 1 or -1
---@return integer|nil line, string|nil err
function M.jump(record, symbols, kind, dir)
  local wanted = ({ section = 3, file = 1, hunk = 6 })[kind]
  if not wanted then
    return nil, 'unknown symbol kind: ' .. tostring(kind)
  end
  local lines = {}
  collect_symbol_lines(symbols, wanted, lines)
  table.sort(lines, function(a, b) return dir > 0 and a < b or dir < 0 and a > b end)
  local win = record and vim.fn.bufwinid(record.buf) or -1
  if win == -1 then
    return nil, 'no view window'
  end
  local current = vim.api.nvim_win_get_cursor(win)[1]
  for _, line in ipairs(lines) do
    if (line - current) * dir > 0 then
      vim.api.nvim_win_set_cursor(win, { line, 0 })
      return line
    end
  end
  return nil, 'no ' .. kind .. ' symbol'
end

--- Install window resize and owned-client shutdown hooks once at client setup.
---@param sessions table<string, table>
function M.setup_lifecycle(sessions)
  vim.api.nvim_create_autocmd({ 'WinResized', 'VimResized' }, {
    group = vim.api.nvim_create_augroup('magit_lsp_resize', { clear = true }),
    callback = M.resize_all,
  })
  vim.api.nvim_create_autocmd('VimLeavePre', {
    group = vim.api.nvim_create_augroup('magit_lsp_leave', { clear = true }),
    callback = function()
      M.close_all()
      for _, session in pairs(sessions) do
        local c = client(session.client_id)
        if c then c:stop(true) end
      end
    end,
  })
end

--- Send every open view's viewport to the server.
function M.resize_all()
  for _, id in ipairs(vim.list_extend({}, M.order)) do
    M.resize(id)
  end
end

return M
