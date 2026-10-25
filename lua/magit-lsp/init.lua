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

--- The status placements this client understands (R-UI-7). They are window
--- implementations only: the server's descriptor placement stays advisory.
local STATUS_PLACEMENTS = { 'replace', 'vsplit', 'split', 'tab' }

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
  if value == nil then
    return 'replace'
  end
  for _, name in ipairs(STATUS_PLACEMENTS) do
    if value == name then
      return name
    end
  end
  vim.notify(
    'magit-lsp: unknown status.placement ' .. vim.inspect(value) .. '; using replace',
    vim.log.levels.WARN
  )
  return 'replace'
end

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
      visit_key = session.visit_key,
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
--- The symbol outline is fetched asynchronously; the cursor moves when it
--- arrives, and a later key never waits for it.
---@param kind string `section`, `file` or `hunk`
---@param dir integer 1 or -1
function M.jump(kind, dir)
  local record = target_record()
  if not record then
    return
  end
  M.symbols(function(symbols)
    if not symbols then
      return
    end
    view.jump(record, symbols, kind, dir)
  end)
end

--- The window a visited file should open in.
---
--- A split or tab placement leaves another window free, so the status view
--- stays visible. When the status view replaced the only buffer there is no
--- second window and the file opens in place; the status buffer stays listed,
--- so `:b`/the buffer list return to it.
---@return integer
local function visit_window()
  local current = vim.api.nvim_get_current_win()
  local windows = vim.api.nvim_list_wins()
  if #windows < 2 then
    return current
  end
  local previous_number = vim.fn.winnr('#')
  if previous_number > 0 then
    local previous = vim.fn.win_getid(previous_number)
    if previous ~= 0 and previous ~= current and vim.api.nvim_win_is_valid(previous) then
      return previous
    end
  end
  for _, win in ipairs(windows) do
    if win ~= current and vim.api.nvim_win_is_valid(win) then
      return win
    end
  end
  return current
end

--- Visit the file (and new-side line) under the cursor.
--- Opens it in another window when the view is split so the view stays
--- visible, and in the status window itself when the view fills the screen.
---@param record table|nil defaults to the focused view
function M.visit(record)
  record = record or view.current()
  if not record then
    return
  end
  vim.lsp.buf.definition({
    on_list = function(options)
      local item = options.items[1]
      if not item or not item.filename then
        return
      end
      local target = visit_window()
      if vim.api.nvim_get_current_win() ~= target then
        vim.api.nvim_set_current_win(target)
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
---
--- Opening is asynchronous: the status view appears when the server answers
--- `view/open`, and the render and keymap that follow it are applied as they
--- arrive. `setup()` itself never waits.
---@param opts table|nil { root_dir?, cmd?, profile?, status?, visit_key?, fold_keys?, menu? }
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

  configure()
  menu.setup(opts.menu)
  options.status.placement = placement_name((opts.status or {}).placement)

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

  -- The placement owns the window: `replace` uses this one, a split or tab
  -- command opens another. `setup()` never splits on its own (R-UI-7).
  session.win = vim.api.nvim_get_current_win()

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

  open_status(session)
end

return M
