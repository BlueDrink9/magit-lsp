local M = {}

local keys = require('magit-lsp.keys')
local keymap = require('magit-lsp.keymap')
local menu = require('magit-lsp.menu')
local prompt = require('magit-lsp.prompt')
local debug_api = require('magit-lsp.debug')

--- The last text the server sent, for clients and tests that need it.
M.lines = {}

--- Notifications that arrived before their `view/open` response named the
--- view, keyed by view id and kept in arrival order.
M.pending_render = {}
M.pending_keymap = {}

--- The view mode the server last reported: `interactive` or `text`.
M.mode = 'interactive'

--- The status buffer's filetype; LSP groups are `@lsp.type.<type>.<filetype>`.
M.filetype = 'magit-lsp'

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
--- Terminals cannot distinguish most `ctrl+shift`/`alt` combinations.
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
local function client_capabilities()
  local capabilities = vim.lsp.protocol.make_client_capabilities()
  capabilities.keymapDelivery = 'dynamic'
  capabilities.keyModifiers = detect_key_modifiers()
  capabilities.prompts = true
  capabilities.placements = { 'main', 'side', 'bottom', 'float' }
  capabilities.editorBridge = false
  return capabilities
end

local LEVELS = {
  info = vim.log.levels.INFO,
  warn = vim.log.levels.WARN,
  error = vim.log.levels.ERROR,
}

local function error_message(err)
  if type(err) == 'table' then
    return tostring(err.message or err)
  end
  return tostring(err)
end

--- Put `text` on the clipboard and the unnamed register.
---
--- `+` has no provider in headless or remote setups, so a failure degrades to
--- the unnamed register and a warning instead of an error.
---@param text string
local function copy_to_clipboard(text)
  local ok = pcall(vim.fn.setreg, '+', text)
  pcall(vim.fn.setreg, '"', text)
  local preview = text:gsub('\n', ' ⏎ ')
  if #preview > 60 then
    preview = preview:sub(1, 57) .. '...'
  end
  if ok then
    vim.notify('Copied: ' .. preview, vim.log.levels.INFO)
  else
    vim.notify('Copied: ' .. preview .. ' (clipboard unavailable)', vim.log.levels.WARN)
  end
end

--- Show one message effect as a notification at its level.
local function notify_effect(effect)
  M.last_error_detail = effect.detail
  local text = effect.text or ''
  if effect.hint then
    text = text .. '\n' .. effect.hint
  end
  vim.notify(text, LEVELS[effect.level] or vim.log.levels.INFO)
end

--- Apply effects from an invoke: messages, clipboard, and view pushes/pops.
local function apply_effects(effects)
  menu.apply_effects(effects)
end

--- Replay a server `passThrough` key natively, without re-triggering our maps.
---@param key string canonical chord, e.g. `Escape` or `g Q`
local function replay(key)
  local lhs = keys.to_lhs(key)
  if not lhs then
    return
  end
  -- `n` means "do not remap", so the client's own buffer-local map is skipped.
  vim.api.nvim_feedkeys(keys.termcodes(lhs), 'n', false)
end

--- Apply the effects of a `view/key`: messages, clipboard and native replay.
local function apply_key_effects(effects)
  menu.apply_effects(effects)
end

--- Standard theme groups for every token type of TASKS.md §3.15.
--- The server names the token types; the client only links them to the
--- colourscheme's own groups, so no colour is ever hard-coded here.
local TOKEN_LINKS = {
  ['magit.section'] = 'Title',
  ['magit.count'] = 'Comment',
  ['magit.file'] = 'Directory',
  ['magit.file.modified'] = 'Changed',
  ['magit.file.added'] = 'Added',
  ['magit.file.deleted'] = 'Removed',
  ['magit.file.renamed'] = 'Changed',
  ['magit.file.untracked'] = 'Comment',
  ['magit.hunk.header'] = 'Special',
  ['magit.diff.added'] = '@diff.plus',
  ['magit.diff.removed'] = '@diff.minus',
  ['magit.diff.context'] = 'Normal',
  ['magit.hash'] = 'Constant',
  ['magit.ref.local'] = 'Type',
  ['magit.ref.remote'] = 'Special',
  ['magit.ref.tag'] = 'Label',
  ['magit.ref.head'] = 'Type',
  ['magit.author'] = 'Identifier',
  ['magit.date'] = 'Comment',
  ['magit.message.warn'] = 'WarningMsg',
  ['magit.message.error'] = 'ErrorMsg',
  ['magit.menu.title'] = 'Title',
  ['magit.menu.group'] = 'Statement',
  ['magit.menu.key'] = 'Keyword',
  ['magit.menu.label'] = 'Normal',
  ['magit.menu.flag'] = 'Special',
  ['magit.menu.flag.off'] = 'MagitMenuFlagOff',
  ['magit.menu.flag.on'] = 'Special',
  ['magit.menu.switch.on'] = 'String',
  ['magit.menu.switch.off'] = 'Comment',
  ['magit.menu.option'] = 'String',
  ['magit.menu.value'] = 'String',
  ['magit.menu.planned'] = 'Comment',
  ['magit.menu.disabled'] = 'MagitMenuDisabled',
  ['magit.menu.pending'] = 'MagitMenuPending',
  ['magit.menu.preview'] = 'Comment',
}

--- Modifier groups are applied above the type, so they should only add
--- attributes (or a background) and never replace a type's foreground.
local MODIFIER_LINKS = {
  staged = 'Bold',
  selected = 'Visual',
  planned = 'Comment',
  disabled = 'Comment',
  bold = 'Bold',
}

--- Link every semantic-token group to a standard theme group.
--- `default = true` keeps a user's own definition; `:colorscheme` clears the
--- groups, so this runs again on `ColorScheme`.
local function define_highlights()
  -- A link cannot add attributes, so the off flag's dim is carried by an
  -- intermediate group; its colour still comes from the colourscheme.
  local comment = vim.api.nvim_get_hl(0, { name = 'Comment' })
  vim.api.nvim_set_hl(0, 'MagitMenuFlagOff', {
    fg = comment.fg,
    bg = comment.bg,
    dim = true,
  })
  -- A menu item that cannot complete the pending prefix, and the prefix of one
  -- that can. Both need an intermediate group because a link cannot add the
  -- dim/bold attributes.
  vim.api.nvim_set_hl(0, 'MagitMenuDisabled', {
    fg = comment.fg,
    bg = comment.bg,
    dim = true,
  })
  local keyword = vim.api.nvim_get_hl(0, { name = 'Keyword' })
  vim.api.nvim_set_hl(0, 'MagitMenuPending', {
    fg = keyword.fg,
    bg = keyword.bg,
    bold = true,
  })
  for token_type, link in pairs(TOKEN_LINKS) do
    local group = string.format('@lsp.type.%s.%s', token_type, M.filetype)
    vim.api.nvim_set_hl(0, group, { link = link, default = true })
  end
  for modifier, link in pairs(MODIFIER_LINKS) do
    local group = string.format('@lsp.mod.%s.%s', modifier, M.filetype)
    vim.api.nvim_set_hl(0, group, { link = link, default = true })
  end
end

local function wait_ready(timeout)
  local client = M.client_id and vim.lsp.get_client_by_id(M.client_id)
  if not client or client.initialized then
    return true
  end
  return vim.wait(timeout or 5000, function()
    return client.initialized
  end, 10)
end

local function request_sync(method, params)
  wait_ready()
  return debug_api.track_request(function()
    for _ = 1, 20 do
      local resp = vim.lsp.buf_request_sync(M.buf, method, params, 5000)
      if resp then
        for _, r in pairs(resp) do
          if r.result ~= nil then
            return r.result, nil
          end
          if r.err then
            return nil, r.err
          end
        end
        return nil, 'empty response for ' .. method
      end
      vim.wait(100)
    end
    return nil, 'timed out waiting for ' .. method
  end)
end

--- Ask Neovim to re-request semantic tokens for the buffer.
--- The server owns highlighting, but it only sends tokens on request, so a
--- re-render has to trigger one. The request is scheduled so the client's
--- outstanding work is visible to `idle` (§A.3).
local function refresh_tokens()
  debug_api.schedule(function()
    if M.buf and vim.api.nvim_buf_is_valid(M.buf) then
      vim.lsp.semantic_tokens.force_refresh(M.buf)
    end
  end)
end

--- The document URI of the view; positional requests must use it verbatim.
local function document_uri()
  return M.uri or vim.uri_from_bufnr(M.buf)
end

--- Enter text mode: the buffer becomes plain editable text.
local function enter_text_mode()
  M.mode = 'text'
  vim.bo[M.buf].modifiable = true
  -- Restore the user's undo setting captured when the buffer was created.
  vim.bo[M.buf].undolevels = M.undolevels
end

--- Leave text mode: drop the undo history and hand the buffer back.
---
--- `undolevels=-1` is set before the server's text replaces the buffer, so `u`
--- cannot restore what the user typed while in text mode.
local function exit_text_mode()
  M.mode = 'interactive'
  vim.bo[M.buf].undolevels = -1
  vim.bo[M.buf].modifiable = false
  vim.bo[M.buf].modified = false
end

--- The only writer of the status buffer: replace `start`..`finish` lines.
--- The view is read-only, so every update goes through here.
---@param buf integer
---@param start integer 0-based first line
---@param finish integer 0-based end line, exclusive; -1 means the last line
---@param lines string[]
local function set_lines(buf, start, finish, lines)
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, start, finish, false, lines)
  vim.bo[buf].modified = false
  -- Renders normally arrive in interactive mode; a stray one in text mode
  -- must not take the buffer back from the user.
  vim.bo[buf].modifiable = M.mode == 'text'
end

--- Apply line-range edits bottom-up so earlier offsets stay valid.
---@param edits table[]
local function apply_edits(edits)
  table.sort(edits, function(a, b)
    return a.startLine > b.startLine
  end)
  for _, edit in ipairs(edits) do
    set_lines(M.buf, edit.startLine, edit.endLine, edit.lines)
  end
end

local function status_window()
  local win = vim.fn.bufwinid(M.buf)
  if win == -1 then
    return nil
  end
  return win
end

--- Move the cursor to the server's successor position (0-based).
---@param cursor table { line = integer, column = integer }
local function set_cursor(cursor)
  local win = status_window()
  if not win then
    return
  end
  local line = math.min(cursor.line, vim.api.nvim_buf_line_count(M.buf) - 1)
  local text = vim.api.nvim_buf_get_lines(M.buf, line, line + 1, false)[1] or ''
  local column = math.min(cursor.column, #text)
  vim.api.nvim_win_set_cursor(win, { line + 1, column })
end

local function ack(version)
  local client = vim.lsp.get_client_by_id(M.client_id)
  if client and M.view_id then
    client:notify('view/rendered', { viewId = M.view_id, version = version })
    debug_api.render_acked(M.view_id, version)
  end
end

local function request_resync()
  local client = vim.lsp.get_client_by_id(M.client_id)
  if not client or not M.view_id then
    return
  end
  debug_api.request_started()
  client:request('view/resync', { viewId = M.view_id }, function(err, result)
    debug_api.request_finished()
    if err or type(result) ~= 'table' then
      return
    end
    M.apply_render(result)
  end, M.buf)
end

--- Apply a `view/render` payload, then acknowledge the version.
---
--- A client that does not hold the edit's base version has lost an update and
--- asks for a full resync instead of applying edits to stale text. A render
--- that arrives before the `view/open` response is buffered, never dropped.
---@param params table
function M.apply_render(params)
  if not params or not params.viewId then
    return
  end
  debug_api.render_received(params.viewId, params.version)
  debug_api.set_pending(params.viewId, params.pending)

  if params.viewId ~= M.view_id then
    -- The notification can beat the response that names the view; keep it
    -- until `open` applies the queue. Renders for any other view are stale.
    if M.view_id == nil then
      local queue = M.pending_render[params.viewId] or {}
      queue[#queue + 1] = params
      M.pending_render[params.viewId] = queue
    end
    return
  end

  -- Leaving text mode: drop undo history before replacing the text, so the
  -- server's version cannot be undone back into the user's edits.
  if params.mode == 'interactive' and M.mode == 'text' then
    exit_text_mode()
  end

  if params.text then
    set_lines(M.buf, 0, -1, params.text)
  elseif params.edits then
    if params.baseVersion ~= M.version then
      request_resync()
      return
    end
    apply_edits(params.edits)
  end

  if params.mode then
    M.mode = params.mode
  end

  M.lines = vim.api.nvim_buf_get_lines(M.buf, 0, -1, false)
  M.version = params.version
  debug_api.render_applied(params.viewId, params.version)
  if params.cursor then
    set_cursor(params.cursor)
  end
  -- Ack before asking for tokens: positional requests resolve against the
  -- acknowledged tree, so the new version must be recorded first.
  ack(params.version)
  refresh_tokens()
end

--- The cursor of the status window, 0-based.
local function current_cursor()
  local win = vim.api.nvim_get_current_win()
  if vim.api.nvim_win_get_buf(win) ~= M.buf then
    win = status_window()
  end
  if not win then
    return { line = 0, column = 0 }
  end
  local position = vim.api.nvim_win_get_cursor(win)
  return { line = position[1] - 1, column = position[2] }
end

--- The current visual selection, 0-based and inclusive.
local function visual_selection()
  local first, last = vim.fn.line('v'), vim.fn.line('.')
  if first == 0 or last == 0 then
    return nil
  end
  return {
    startLine = math.min(first, last) - 1,
    endLine = math.max(first, last) - 1,
  }
end

--- The editor mode a key press should report.
local function editor_mode()
  local mode = vim.fn.mode()
  if mode == 'v' or mode == 'V' or mode == '\22' then
    return 'visual'
  end
  return 'normal'
end

--- Send one key's canonical steps and apply the answer.
---
--- A chord is sent as its steps in order; the server tracks the pending
--- prefix. Cursor, mode and selection are fixed at press time so every step
--- resolves against the same context.
---@param steps string[] canonical key steps
function M.press(steps)
  if not M.view_id then
    return
  end
  debug_api.key_pressed(steps)
  local mode = editor_mode()
  local selection = mode == 'visual' and visual_selection() or nil
  local cursor = current_cursor()
  local effects = {}
  for _, step in ipairs(steps) do
    debug_api.key_sent(step, M.view_id)
    local resp, err = request_sync('view/key', {
      viewId = M.view_id,
      version = M.version or 0,
      key = step,
      editorMode = mode,
      cursor = cursor,
      selection = selection,
    })
    if not resp then
      vim.notify('magit-lsp key ' .. step .. ' failed: ' .. error_message(err), vim.log.levels.WARN)
      return
    end
    vim.list_extend(effects, resp.effects or {})
  end
  apply_key_effects(effects)
end

--- Run a command from a client-side fold mirror.
local function run_command(command, args)
  return M.invoke(command, { args = args })
end

--- Run a command against the open view.
---@param command string
---@param opts table|nil { cursor?: table, selection?: boolean|table, args?: table }
---@return table|nil response
function M.invoke(command, opts)
  opts = opts or {}
  if not M.view_id then
    return nil, 'no view is open'
  end

  local cursor = opts.cursor or current_cursor()
  local selection = opts.selection
  if selection == true then
    selection = visual_selection()
  end

  local before = M.version
  local resp, err = request_sync('view/invoke', {
    viewId = M.view_id,
    command = command,
    cursor = cursor,
    selection = selection,
    args = opts.args,
  })
  if not resp then
    M.last_error_detail = type(err) == 'table' and err.data and err.data.detail or nil
    vim.notify('magit-lsp ' .. command .. ' failed: ' .. error_message(err), vim.log.levels.WARN)
    return nil, err
  end

  apply_effects(resp.effects)
  local opened = false
  for _, effect in ipairs(resp.effects or {}) do
    if effect.type == 'pushView' or effect.type == 'openView' then
      opened = true
    end
  end
  if not opened and M.version == before then
    vim.wait(5000, function()
      return M.version ~= before
    end, 10)
  end
  return resp
end

--- Apply a `view/keymap` notification atomically (§3.9).
---
--- The map set is the server's bindings translated to Neovim keys plus the
--- client-side fold mirrors and, for the status scope, the temporary visit
--- exception until `common.visit` is implemented. A keymap that arrives before
--- the `view/open` response is buffered and applied when the view exists.
---@param notice table `view/keymap` params
function M.apply_keymap(notice)
  if type(notice) ~= 'table' or not notice.viewId then
    return
  end
  if not M.buf or not vim.api.nvim_buf_is_valid(M.buf) then
    return
  end
  if notice.viewId ~= M.view_id then
    if M.view_id == nil then
      local queue = M.pending_keymap[notice.viewId] or {}
      queue[#queue + 1] = notice
      M.pending_keymap[notice.viewId] = queue
    end
    return
  end
  M.last_keymap = notice

  if notice.mode == 'text' then
    enter_text_mode()
  elseif M.mode == 'text' then
    exit_text_mode()
  end

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
      visit = { lhs = M.visit_key, fn = function()
        M.visit()
      end }
    end
  end

  keymap.apply(M.buf, notice, { press = M.press, command = run_command }, {
    visit = visit,
    fold_keys = M.fold_keys,
  })
  debug_api.keymap_received(notice.viewId, notice)
  M.capture = notice.capture == true
  M.keymap_ready = true
end

--- Open the status view in the current buffer.
---@return table|nil descriptor
function M.open()
  M.keymap_ready = false
  M.last_keymap = nil
  local descriptor, err = request_sync('view/open', {
    kind = 'status',
    args = { repo_path = M.root },
    viewport = {
      cols = vim.o.columns,
      rows = vim.o.lines,
    },
  })
  if not descriptor then
    vim.notify('magit-lsp open failed: ' .. error_message(err), vim.log.levels.ERROR)
    return nil, err
  end

  local existing = vim.fn.bufnr(descriptor.uri)
  if existing ~= -1 and existing ~= M.buf then
    vim.api.nvim_buf_delete(existing, { force = true })
  end

  M.view_id = descriptor.id
  M.uri = descriptor.uri
  M.version = nil
  vim.api.nvim_buf_set_name(M.buf, descriptor.uri)
  debug_api.view_open(M.view_id, 'status', M.buf)

  -- Notifications can beat the `view/open` response. Apply what was buffered,
  -- in arrival order, now that the view has a name; without this the first
  -- render would be lost and the buffer would stay empty.
  local renders = M.pending_render[M.view_id]
  M.pending_render[M.view_id] = nil
  for _, render in ipairs(renders or {}) do
    M.apply_render(render)
  end
  local keymaps = M.pending_keymap[M.view_id]
  M.pending_keymap[M.view_id] = nil
  for _, notice in ipairs(keymaps or {}) do
    M.apply_keymap(notice)
  end

  -- The initial render follows the response as a notification.
  if not vim.wait(5000, function()
    return M.version ~= nil
  end, 10) then
    vim.notify('magit-lsp did not render the view', vim.log.levels.ERROR)
    return nil, 'no initial render'
  end

  -- `view/keymap` follows the render; the client only holds keys the server
  -- sent, so wait for the first one before reporting the view open.
  vim.wait(5000, function()
    return M.keymap_ready == true
  end, 10)
  return descriptor
end

--- Ask the server to re-read the repository and re-render.
function M.refresh()
  return M.invoke('common.refresh')
end

--- Send the current viewport to the server.
function M.resize()
  local win = status_window()
  local client = M.client_id and vim.lsp.get_client_by_id(M.client_id)
  if not win or not client or not M.view_id then
    return
  end
  client:notify('view/resize', {
    viewId = M.view_id,
    cols = vim.api.nvim_win_get_width(win),
    rows = vim.api.nvim_win_get_height(win),
  })
end

--- LSP `SymbolKind` values for the server's section/file/hunk outline.
local SYMBOL_KINDS = { section = 3, file = 1, hunk = 6 }

--- Fetch the document's hierarchical symbol tree (section → file → hunk).
function M.symbols()
  local symbols, err = request_sync('textDocument/documentSymbol', {
    textDocument = { uri = document_uri() },
  })
  if not symbols then
    return nil, err
  end
  return symbols
end

local function collect_symbol_lines(symbols, kind, lines)
  for _, symbol in ipairs(symbols or {}) do
    if symbol.kind == kind then
      table.insert(lines, symbol.range.start.line + 1)
    end
    collect_symbol_lines(symbol.children, kind, lines)
  end
end

--- Move the cursor to the next/previous symbol of `kind` (`dir` is 1 or -1).
function M.jump(kind, dir)
  local symbols, err = M.symbols()
  if not symbols then
    return nil, err
  end
  local wanted = SYMBOL_KINDS[kind]
  if not wanted then
    return nil, 'unknown symbol kind: ' .. tostring(kind)
  end
  local lines = {}
  collect_symbol_lines(symbols, wanted, lines)
  table.sort(lines)

  local cur = vim.fn.line('.')
  if dir > 0 then
    for _, line in ipairs(lines) do
      if line > cur then
        vim.api.nvim_win_set_cursor(0, { line, 0 })
        return line
      end
    end
  else
    for i = #lines, 1, -1 do
      if lines[i] < cur then
        vim.api.nvim_win_set_cursor(0, { lines[i], 0 })
        return lines[i]
      end
    end
  end
  return nil, 'no ' .. kind .. ' symbol'
end

--- Visit the file (and new-side line) under the cursor.
--- Opens it in the window the status view was split from, so the status
--- buffer stays visible.
function M.visit()
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
  local detail = M.last_error_detail
  if not detail or detail == '' then
    vim.notify('magit-lsp: no error detail to show', vim.log.levels.INFO)
    return
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].buftype = 'nofile'
  vim.bo[buf].bufhidden = 'hide'
  vim.bo[buf].swapfile = false
  vim.api.nvim_buf_set_name(buf, 'magit-lsp://error')
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(detail, '\n', { plain = true }))
  vim.cmd('botright split')
  vim.api.nvim_win_set_buf(0, buf)
end

local function enable_folding(win)
  -- The server owns section visibility; native folding must never interfere.
  vim.wo[win].foldmethod = 'manual'
  vim.wo[win].foldenable = false
end

-- Text mode makes the buffer editable, so this hint only covers interactive mode.
local READ_ONLY_HINT =
  'Read-only view. Press the text-mode key (ctrl+t) to edit or copy as plain text.'

local WRITE_COMMANDS = { w = true, wr = true, wri = true, writ = true, write = true }

--- True when `cmdline` is a bare `:w`/`:write` (any abbreviation, optional `!`).
local function is_write_command(cmdline)
  local cmd = cmdline:match('^%s*(%a+)!?%s*$')
  return cmd ~= nil and WRITE_COMMANDS[cmd] == true
end

--- Show the read-only hint once per session.
local function notify_read_only()
  if M.read_only_hint_shown then
    return
  end
  M.read_only_hint_shown = true
  vim.notify(READ_ONLY_HINT, vim.log.levels.INFO)
end

--- Make `buf` a read-only view of server-owned text.
---@param buf integer
local function configure_read_only(buf)
  vim.api.nvim_create_autocmd('BufWriteCmd', {
    buffer = buf,
    callback = function()
      vim.bo[buf].modified = false
      notify_read_only()
    end,
  })

  -- `nofile` buffers reject `:w` with E382 before BufWriteCmd runs, so also
  -- swallow a bare write at the command line. Clearing the command line
  -- cancels it; pcall because that only works while cmdline mode is active.
  --
  -- Native E21 edit failures have no autocmd to observe, so the hint is shown
  -- on the first attempted write; text mode also never writes to disk.
  vim.api.nvim_create_autocmd('CmdlineLeave', {
    buffer = buf,
    callback = function()
      if vim.fn.getcmdtype() ~= ':' or not is_write_command(vim.fn.getcmdline()) then
        return
      end
      pcall(vim.fn.setcmdline, '')
      vim.bo[buf].modified = false
      notify_read_only()
    end,
  })
end

local function fresh_buffer()
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].buftype = 'nofile'
  vim.bo[buf].bufhidden = 'hide'
  vim.bo[buf].swapfile = false
  vim.bo[buf].modifiable = false
  -- `readonly` is deliberately unset: Vim warns W10 on every programmatic
  -- write to a readonly buffer. `modifiable=false` and `buftype=nofile`
  -- already reject edits and writes.
  -- Text mode restores the user's undo setting; the interactive view disables
  -- undo because a refresh replaces the text wholesale.
  M.undolevels = vim.o.undolevels
  vim.bo[buf].undolevels = -1
  vim.bo[buf].modified = false
  -- A dedicated filetype keeps the `@lsp.type.*.<filetype>` group names stable.
  vim.bo[buf].filetype = M.filetype
  configure_read_only(buf)
  return buf
end

--- Start the server and open the status view.
---@param opts table|nil { root_dir?: string, cmd?: string[], profile?: string, visit_key?: string, fold_keys?: table<string, string> }
---@return table|nil descriptor
function M.setup(opts)
  opts = opts or {}
  M.root = opts.root_dir or vim.fn.getcwd()
  M.profile = opts.profile or 'vim'
  M.visit_key = opts.visit_key or '<CR>'
  M.fold_keys = opts.fold_keys or {}
  M.keyModifiers = detect_key_modifiers()
  local cmd = opts.cmd or default_cmd()

  debug_api.configure()
  define_highlights()

  -- `:colorscheme` runs `hi clear`, so re-apply the links afterwards.
  vim.api.nvim_create_autocmd('ColorScheme', {
    group = vim.api.nvim_create_augroup('magit_lsp_highlights', { clear = true }),
    callback = define_highlights,
  })

  vim.api.nvim_create_user_command('MagitLspErrorDetail', M.error_detail, {
    desc = 'Show the last magit-lsp error detail',
    force = true,
  })

  -- The server sends render and keymap updates as notifications. A menu
  -- render carries the structured model, and menu keymaps use `menu.*`.
  vim.lsp.handlers['view/render'] = function(_, result)
    if result and result.menu ~= nil then
      menu.apply_render(result)
    else
      M.apply_render(result)
    end
  end
  vim.lsp.handlers['view/keymap'] = function(_, result)
    if result and (menu.owns(result.viewId) or (result.scope or ''):match('^menu%.')) then
      menu.apply_keymap(result)
    else
      M.apply_keymap(result)
    end
  end

  -- Prompts arrive as notifications; the client shows the right widget and
  -- answers with `view/promptRespond`.
  vim.lsp.handlers['view/prompt'] = function(_, result)
    prompt.handle(result)
  end

  menu.setup(opts.menu)

  M.buf = fresh_buffer()
  M.lines = {}
  M.pending_render = {}
  M.pending_keymap = {}
  M.mode = 'interactive'
  M.version = nil
  M.view_id = nil
  M.uri = nil
  M.keymap_ready = false
  M.last_keymap = nil
  keymap.clear(M.buf)

  vim.cmd('vsplit')
  local win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, M.buf)
  enable_folding(win)

  M.client_id = vim.lsp.start({
    name = 'magit-lsp',
    cmd = cmd,
    root_dir = M.root,
    capabilities = client_capabilities(),
    init_options = { profile = M.profile },
  }, { bufnr = M.buf })

  if not M.client_id then
    vim.notify('Failed to start magit-lsp server', vim.log.levels.ERROR)
    return nil, 'failed to start client'
  end

  prompt.configure({ client_id = M.client_id })

  menu.configure({
    client_id = M.client_id,
    filetype = M.filetype,
    notify = notify_effect,
    clipboard = copy_to_clipboard,
    replay = replay,
    status_win = function()
      local win = vim.fn.bufwinid(M.buf)
      if win == -1 then
        return nil
      end
      return win
    end,
  })

  -- A user can add fold mappings while the server is running; re-scan them
  -- whenever the view regains focus.
  vim.api.nvim_create_autocmd('BufEnter', {
    buffer = M.buf,
    callback = function()
      if M.last_keymap then
        M.apply_keymap(M.last_keymap)
      end
    end,
  })

  vim.api.nvim_create_autocmd({ 'WinResized', 'VimResized' }, {
    group = vim.api.nvim_create_augroup('magit_lsp_resize', { clear = true }),
    callback = M.resize,
  })

  vim.api.nvim_create_autocmd('VimLeavePre', {
    callback = function()
      menu.close_all()
      local client = M.client_id and vim.lsp.get_client_by_id(M.client_id)
      if client then
        client:stop(true)
      end
    end,
  })

  return M.open()
end

return M
