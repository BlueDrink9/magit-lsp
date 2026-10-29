--- Neovim client entry point.
---
--- `setup()` starts one LSP client per repository and opens its status view.
--- All view lifetime, rendering and key handling lives in `view.lua`; this
--- module only owns configuration, the LSP transport, prompts and the small
--- public API used by scripts and tests.

local buffer = require('magit-lsp.buffer')
local config = require('magit-lsp.config')
local effects = require('magit-lsp.effects')
local highlights = require('magit-lsp.highlights')
local view = require('magit-lsp.view')
local menu = require('magit-lsp.menu')
local edit = require('magit-lsp.edit')
local prompt = require('magit-lsp.prompt')
local hooks = require('magit-lsp.hooks')

local M = {}

--- Open sessions by repository root.
---@type table<string, table>
M.sessions = {}

--- Client-only options; nothing here is sent to the server.
local options = { status = { placement = 'replace' } }

--- Show `buf` in `win`, hiding an unsaved buffer instead of failing with E37.
---@param win integer
---@param buf integer
local function show_buffer(win, buf)
  if vim.api.nvim_win_get_buf(win) == buf then
    return
  end
  if pcall(vim.api.nvim_win_set_buf, win, buf) then
    return
  end
  -- The window holds an unsaved buffer and 'hidden' is off; hide it rather
  -- than abort the status view.
  vim.api.nvim_win_call(win, function()
    pcall(vim.cmd, 'hide')
  end)
  vim.api.nvim_win_set_buf(win, buf)
end

--- Build a status placement: `command` opens its window, nil reuses `opts.win`.
---@param command string|nil `vsplit`, `split` or `tabnew`
---@return fun(view: table, opts: table|nil): integer
local function status_placement(command)
  return function(place_view, opts)
    local win = opts and opts.win
    if command then
      vim.cmd(command)
      win = vim.api.nvim_get_current_win()
    elseif not win or not vim.api.nvim_win_is_valid(win) then
      win = vim.api.nvim_get_current_win()
    end
    show_buffer(win, place_view.buf)
    buffer.enable_folding(win)
    return win
  end
end

--- Validate the client-only `status.placement` option.
---@param value any
---@return string
local function placement_name(value)
  return config.placement(value)
end

local function error_message(err)
  if type(err) == 'table' then
    return tostring(err.message or err)
  end
  return tostring(err)
end

--- The session owning an LSP client id.
---@param client_id integer|nil
---@return table|nil
local function session_for_client(client_id)
  for _, session in pairs(M.sessions) do
    if session.client_id == client_id then
      return session
    end
  end
  return nil
end

--- The session owning a view record.
---@param record table|nil
---@return table|nil
local function session_for_view(record)
  if not record then
    return nil
  end
  return M.sessions[record.root] or session_for_client(record.client_id)
end

--- The status record of the current, or the only, session.
---@param root string|nil
---@return table|nil
function M.status_view(root)
  if root then
    return view.status(root)
  end
  local current = view.current()
  if current and current.root then
    local record = view.status(current.root)
    if record then
      return record
    end
  end
  return view.status(nil)
end

--- Open the status view for `session` in its boot buffer.
---
--- The server names the view in its `view/open` response, so the request is
--- asynchronous: the record appears when the descriptor does, and the render
--- and keymap that follow it are applied by the view manager.
---@param session table
function open_status(session)
  if not vim.api.nvim_buf_is_valid(session.buf) then session.buf = buffer.fresh() end
  vim.lsp.buf_attach_client(session.buf, session.client_id)
  view.request(session.buf, session.client_id, 'view/open', {
    kind = 'status',
    args = { repo_path = session.root },
    viewport = { cols = vim.o.columns, rows = vim.o.lines },
  }, function(descriptor, err)
    if not descriptor then
      vim.notify('magit-lsp open failed: ' .. error_message(err), vim.log.levels.ERROR)
      return
    end
    local record = view.open(descriptor, {
      buf = session.buf,
      win = session.win,
      client_id = session.client_id,
      root = session.root,
      fold_keys = session.fold_keys,
      placement = options.status.placement,
    })
    if record then
      session.win = record.win
    end
  end)
end

--- Open a status view for `root`, starting its session when needed.
---
--- Opening is asynchronous; the view is focused once the server has described
--- it. Calling this for an already-open root focuses it immediately.
---@param root string|nil defaults to the current session or cwd
function M.open(root)
  local record = M.status_view(root)
  if record then
    view.focus(record.key)
    return record.descriptor
  end
  local session = root and M.sessions[root] or nil
  if not session then
    local current = view.current()
    session = current and session_for_view(current) or nil
  end
  if not session then
    return nil, 'no magit-lsp session is open'
  end
  open_status(session)
end

--- Send one key chord to the focused view (or `id`); the answer applies later.
---@param steps string[]
---@param id string|nil
function M.press(steps, id)
  return view.press(steps, id)
end

--- Run a command against the focused view (or `id`), applied asynchronously.
---@param command string
---@param opts table|nil { cursor?, selection?, args? }
---@param id string|nil
---@return boolean|nil queued, string|nil err
function M.invoke(command, opts, id)
  return view.invoke(command, opts, id)
end

--- Ask the server to re-read the repository and re-render.
function M.refresh()
  return M.invoke('common.refresh')
end

--- Send every open view's viewport to the server.
function M.resize()
  view.resize_all()
end

--- The record used for positional requests.
---@return table|nil
local function target_record()
  return view.current() or M.status_view()
end

--- Fetch the current view's hierarchical symbol tree asynchronously.
---
--- `on_result(symbols, err)` runs after the server answers; navigation is
--- server-owned, so no rendered text is parsed and nothing waits on the reply.
---@param on_result fun(symbols: table|nil, err: any)
---@return boolean|nil queued
function M.symbols(on_result)
  local record = target_record()
  if not record then
    if on_result then
      on_result(nil, 'no view is open')
    end
    return nil
  end
  view.request(record.buf, record.client_id, 'textDocument/documentSymbol', {
    textDocument = { uri = view.uri(record) },
  }, on_result or function() end)
  return true
end

--- Move the cursor to the next/previous symbol of `kind` (`dir` is 1 or -1).
---
--- Navigation is resolved by the server against its structured rendered view.
---@param kind string `section`, `file` or `hunk`
---@param dir integer 1 or -1
function M.jump(kind, dir)
  return M.invoke('common.jump', { args = { kind = kind, dir = dir } })
end

--- Open the detail of the last error in a scratch split.
function M.error_detail()
  effects.error_detail()
end

M.highlights = highlights
M.effects = effects

local configured = false

--- Install handlers and shared configuration once per Neovim instance.
local function configure()
  if configured then
    return
  end
  configured = true

  hooks.configure()

  view.configure({
    notify = effects.notify,
    clipboard = effects.copy,
    replay = effects.replay,
    visit_file = effects.visit_file,
    edit_document = function(effect, source)
      edit.open(effect, source)
    end,
    set_error_detail = function(detail)
      effects.last_error_detail = detail
    end,
  })

  -- The status view's window is the client's choice (R-UI-7); these are window
  -- implementations, not server descriptor placements.
  view.register_placement('replace', { place = status_placement(nil) })
  view.register_placement('vsplit', { place = status_placement('vsplit') })
  view.register_placement('split', { place = status_placement('split') })
  view.register_placement('tab', { place = status_placement('tabnew') })

  edit.setup()
  highlights.setup(buffer.filetype)

  vim.api.nvim_create_user_command('MagitLspErrorDetail', M.error_detail, {
    desc = 'Show the last magit-lsp error detail',
    force = true,
  })

  -- Every view kind shares one render/keymap path: the view id decides which
  -- record (and therefore which buffer) receives the payload.
  vim.lsp.handlers['view/render'] = function(_, result, ctx)
    view.apply_render(result, ctx and ctx.client_id)
  end
  vim.lsp.handlers['view/keymap'] = function(_, result, ctx)
    view.apply_keymap(result, ctx and ctx.client_id)
  end
  vim.lsp.handlers['view/prompt'] = function(_, result, ctx)
    prompt.handle(result, ctx and ctx.client_id)
  end
  -- Detached Git work (a commit) still has to reach the user after the request
  -- that started it answered, so keep the client that produced it.
  vim.lsp.handlers['view/effects'] = function(_, result, ctx)
    if not result then
      return
    end
    local session = session_for_client(ctx and ctx.client_id)
    view.apply_effects(result.effects, {
      client_id = ctx and ctx.client_id or nil,
      root = session and session.root,
    })
  end

  view.setup_lifecycle(M.sessions)
end

--- Start an LSP client for one repository and open its status view.
--- Calling it again for an already-open root focuses that status view.
---
--- Opening is asynchronous: the status view appears when the server answers
--- `view/open`, and the render and keymap that follow it are applied as they
--- arrive. `setup()` itself never waits.
---@param opts table|nil { root_dir?, cmd?, profile?, status?, fold_keys?, menu? }
---@return table|nil descriptor, any err the descriptor of an already-open status view
function M.setup(opts)
  opts = opts or {}
  local root = opts.root_dir or vim.fn.getcwd()
  local existing = M.sessions[root]
  if existing then
    local record = view.status(root)
    if record then
      view.focus(record.key)
      return record.descriptor
    end
  end

  local cmd, cmd_error = config.server_command(opts.cmd)
  if not cmd then
    vim.notify(cmd_error, vim.log.levels.ERROR)
    return nil, cmd_error
  end

  configure()
  menu.setup(opts.menu)
  options.status.placement = placement_name((opts.status or {}).placement)

  local profile = opts.profile or 'vim'
  local fold_keys = opts.fold_keys or {}
  local session = existing or {
    root = root,
    profile = profile,
    fold_keys = fold_keys,
    buf = buffer.fresh(),
  }
  M.sessions[root] = session
  session.fold_keys = fold_keys
  if not vim.api.nvim_buf_is_valid(session.buf) then session.buf = buffer.fresh() end

  -- The placement owns the window: `replace` uses this one, a split or tab
  -- command opens another. `setup()` never splits on its own (R-UI-7).
  session.win = vim.api.nvim_get_current_win()

  if not session.client_id then
    session.client_id = vim.lsp.start({
      name = 'magit-lsp',
      cmd = cmd,
      root_dir = root,
      capabilities = config.client_capabilities(),
      init_options = { profile = profile },
      on_init = function(_, result)
        local capabilities = result and result.capabilities
        local provider = capabilities and capabilities.semanticTokensProvider
        highlights.apply_legend(buffer.filetype, provider and provider.legend)
      end,
      on_exit = function()
        vim.schedule(function() edit.disconnected(session.client_id) end)
      end,
    }, { bufnr = session.buf })
    if not session.client_id then
      vim.notify('Failed to start magit-lsp server', vim.log.levels.ERROR)
      return nil, 'failed to start client'
    end
  end

  open_status(session)
end

return M
