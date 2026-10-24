--- Neovim client entry point.
---
--- `setup()` starts one LSP client per repository and opens its status view.
--- All view lifetime, rendering and key handling lives in `view.lua`; this
--- module only owns configuration, the LSP transport, prompts and the small
--- public API used by scripts and tests.

local buffer = require('magit-lsp.buffer')
local effects = require('magit-lsp.effects')
local highlights = require('magit-lsp.highlights')
local view = require('magit-lsp.view')
local menu = require('magit-lsp.menu')
local edit = require('magit-lsp.edit')
local prompt = require('magit-lsp.prompt')
local debug_api = require('magit-lsp.debug')

local M = {}

--- Open sessions by repository root.
---@type table<string, table>
M.sessions = {}

local function project_root()
  local source = debug.getinfo(1, 'S').source
  local path = source:sub(2)
  path = vim.fn.fnamemodify(path, ':h') -- lua/magit-lsp
  path = vim.fn.fnamemodify(path, ':h') -- lua
  return vim.fn.fnamemodify(path, ':h') -- project root
end

local function default_cmd()
  local bin = project_root() .. '/target/debug/magit-lsp'
  if vim.fn.filereadable(bin) == 1 then
    return { bin }
  end
  return { 'cargo', 'run', '--manifest-path', project_root() .. '/Cargo.toml' }
end

--- How faithfully this client can report key modifiers (§3.8).
local function detect_key_modifiers()
  if vim.fn.has('gui_running') == 1 then
    return 'full'
  end
  local program = vim.env.TERM_PROGRAM
  if program == 'iTerm.app' or program == 'WezTerm' or vim.env.KITTY_WINDOW_ID then
    return 'full'
  end
  return 'basic'
end

--- The client capabilities the server reads, plus the key-delivery contract.
---@return table
local function client_capabilities()
  local capabilities = vim.lsp.protocol.make_client_capabilities()
  capabilities.keymapDelivery = 'dynamic'
  capabilities.keyModifiers = detect_key_modifiers()
  capabilities.prompts = true
  capabilities.placements = { 'main', 'side', 'bottom', 'float' }
  capabilities.editorBridge = false
  return capabilities
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
---@param session table
---@return table|nil record, any err
local function open_status(session)
  if not vim.api.nvim_buf_is_valid(session.buf) then session.buf = buffer.fresh() end
  vim.lsp.buf_attach_client(session.buf, session.client_id)
  local descriptor, err = view.sync(session.buf, session.client_id, 'view/open', {
    kind = 'status',
    args = { repo_path = session.root },
    viewport = { cols = vim.o.columns, rows = vim.o.lines },
  })
  if not descriptor then
    vim.notify('magit-lsp open failed: ' .. error_message(err), vim.log.levels.ERROR)
    return nil, err
  end

  local record = view.open(descriptor, {
    buf = session.buf,
    win = session.win,
    client_id = session.client_id,
    root = session.root,
    visit_key = session.visit_key,
    fold_keys = session.fold_keys,
  })
  if not record then
    return nil, 'view/open returned no view'
  end

  -- The initial render is a notification that follows the response; a render
  -- that beat it is already in the pending queue. Either way it must be
  -- applied in the view's own buffer, never a global one. `view.open` already
  -- applies both queues and flushes anything deferred by the sync request.
  if not vim.wait(5000, function()
    return record.version ~= nil
  end, 10) then
    vim.notify('magit-lsp did not render the view', vim.log.levels.ERROR)
    return nil, 'no initial render'
  end
  -- `view/keymap` follows the render. Holding keys the server never sent is a
  -- bug, so wait for the first set before reporting the view open.
  vim.wait(5000, function()
    return record.keymap_ready == true
  end, 10)
  return record
end

--- Open a status view for `root`, starting its session when needed.
---@param root string|nil defaults to the current session or cwd
---@return table|nil descriptor, any err
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
  record = open_status(session)
  return record and record.descriptor or nil, record and nil or 'open failed'
end

--- Send one key chord to the focused view (or `id`) and apply the answer.
---@param steps string[]
---@param id string|nil
function M.press(steps, id)
  return view.press(steps, id)
end

--- Run a command against the focused view (or `id`).
---@param command string
---@param opts table|nil { cursor?, selection?, args? }
---@param id string|nil
---@return table|nil response
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

--- Fetch the current view's hierarchical symbol tree.
---@return table|nil symbols, any err
function M.symbols()
  local record = target_record()
  if not record then
    return nil, 'no view is open'
  end
  return view.sync(record.buf, record.client_id, 'textDocument/documentSymbol', {
    textDocument = { uri = view.uri(record) },
  })
end

--- Move the cursor to the next/previous symbol of `kind` (`dir` is 1 or -1).
---@param kind string `section`, `file` or `hunk`
---@param dir integer 1 or -1
---@return integer|nil line, any err
function M.jump(kind, dir)
  local symbols, err = M.symbols()
  if not symbols then
    return nil, err
  end
  return view.jump(target_record(), symbols, kind, dir)
end

--- Visit the file (and new-side line) under the cursor.
--- Opens it in the window the view was split from, so the view stays visible.
---@param record table|nil defaults to the focused view
function M.visit(record)
  record = record or view.current()
  if not record then
    return
  end
  local win = vim.api.nvim_get_current_win()
  vim.lsp.buf.definition({
    on_list = function(options)
      local item = options.items[1]
      if not item or not item.filename then
        return
      end
      if vim.api.nvim_get_current_win() == win then
        vim.cmd('wincmd p')
      end
      vim.cmd('edit ' .. vim.fn.fnameescape(item.filename))
      vim.api.nvim_win_set_cursor(0, { item.lnum, math.max((item.col or 1) - 1, 0) })
    end,
  })
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

  debug_api.configure()

  view.configure({
    notify = effects.notify,
    clipboard = effects.copy,
    replay = effects.replay,
    visit = function(record)
      M.visit(record)
    end,
    edit_document = function(effect, source)
      edit.open(effect, source)
    end,
    set_error_detail = function(detail)
      effects.last_error_detail = detail
    end,
  })

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
    prompt.configure({ client_id = ctx and ctx.client_id })
    prompt.handle(result)
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
---@param opts table|nil { root_dir?, cmd?, profile?, visit_key?, fold_keys?, menu? }
---@return table|nil descriptor, any err
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

  configure()
  menu.setup(opts.menu)

  local profile = opts.profile or 'vim'
  local visit_key = opts.visit_key or '<CR>'
  local fold_keys = opts.fold_keys or {}
  local cmd = opts.cmd or default_cmd()
  local session = existing or {
    root = root,
    profile = profile,
    visit_key = visit_key,
    fold_keys = fold_keys,
    buf = buffer.fresh(),
  }
  M.sessions[root] = session
  session.visit_key = visit_key
  session.fold_keys = fold_keys
  if not vim.api.nvim_buf_is_valid(session.buf) then session.buf = buffer.fresh() end

  vim.cmd('vsplit')
  session.win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(session.win, session.buf)
  buffer.enable_folding(session.win)

  if not session.client_id then
    session.client_id = vim.lsp.start({
      name = 'magit-lsp',
      cmd = cmd,
      root_dir = root,
      capabilities = client_capabilities(),
      init_options = { profile = profile },
    }, { bufnr = session.buf })
    if not session.client_id then
      vim.notify('Failed to start magit-lsp server', vim.log.levels.ERROR)
      return nil, 'failed to start client'
    end
    prompt.configure({ client_id = session.client_id })
  end

  local record, err = open_status(session)
  return record and record.descriptor or nil, err
end

return M
