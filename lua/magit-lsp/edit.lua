--- Managed commit drafts: one explicit full-sync stream, versioned barriers and
--- positively classified native close intent. No Git path is opened or written.
local view = require('magit-lsp.view')
local hooks = require('magit-lsp.hooks')
local M = {}
local progress = {}

local function live(record)
  return record and view.views[record.key] == record and not record.closing
    and vim.api.nvim_buf_is_valid(record.buf)
end

local function text(record)
  local lines = vim.api.nvim_buf_get_lines(record.buf, 0, -1, false)
  return table.concat(lines, '\n') .. (vim.bo[record.buf].endofline and '\n' or '')
end

local function replace(record, value)
  record.applying = true
  local lines = vim.split(value, '\n', { plain = true })
  local eol = value:sub(-1) == '\n'
  if eol then table.remove(lines) end
  if #lines == 0 then lines = { '' } end
  vim.api.nvim_buf_set_lines(record.buf, 0, -1, false, lines)
  vim.bo[record.buf].endofline = eol
  record.text = value
  record.applying = false
end

local function fail(record, err)
  record.generation = (record.generation or 0) + 1
  record.sync_error = type(err) == 'table' and err.message or tostring(err)
  record.busy = false
  if live(record) then vim.bo[record.buf].modifiable = true end
  vim.notify('magit-lsp: commit draft: ' .. record.sync_error
    .. '. Your text is retained; retry :write or abort explicitly.', vim.log.levels.ERROR)
end

local function notify(record, method, params)
  local client = vim.lsp.get_client_by_id(record.client_id)
  if not client or not client:notify(method, params) then
    fail(record, 'the draft connection is unavailable')
    return false
  end
  return true
end

--- Flush pending native text exactly once, using strictly increasing versions.
---@param record table
---@return boolean
function M.flush(record)
  if not live(record) or record.disconnected then return false end
  local value = text(record)
  if #value > 1024 * 1024 then
    fail(record, 'draft exceeds the 1 MiB UTF-8 limit; shorten it before saving')
    return false
  end
  if value ~= record.text or record.sync_error then
    record.version = record.version + 1
    record.text = value
    if not notify(record, 'textDocument/didChange', {
      textDocument = { uri = record.uri, version = record.version },
      contentChanges = { { text = value } },
    }) then return false end
  end
  return true
end

local function params(record, version)
  return { runId = record.id, uri = record.uri, version = version or record.version }
end

local function barrier(record, method, payload, expected, done)
  local generation, base = record.generation, record.version
  view.request(record.buf, record.client_id, method, payload, function(result, err)
    if not live(record) or record.disconnected or record.generation ~= generation then return end
    if err or type(result) ~= 'table' or result.type ~= expected
      or result.runId ~= record.id or result.uri ~= record.uri
      or result.version ~= payload.version or record.version ~= base then
      fail(record, err or 'unexpected draft acknowledgement')
      return
    end
    done(result)
  end)
end

local function unlock(record)
  record.busy = false
  if live(record) then vim.bo[record.buf].modifiable = true end
end

local function finish(record, reason)
  local payload = params(record, record.saved_version)
  payload.reason = reason
  barrier(record, 'draft/finish', payload, 'draft/finished', function()
    notify(record, 'textDocument/didClose', { textDocument = { uri = record.uri } })
    view.close(record.key, false)
    view.focus(record.base)
  end)
end

local function checkpoint(record, done)
  local value = record.text
  barrier(record, 'draft/save', params(record), 'draft/checkpointed', function()
    record.saved_version = record.version
    record.saved_text = value
    record.sync_error = nil
    vim.bo[record.buf].modified = text(record) ~= value
    done()
  end)
end

local function begin(record)
  if not live(record) or record.busy or record.disconnected then return false end
  if not M.flush(record) then return false end
  record.busy = true
  vim.bo[record.buf].modifiable = false
  return true
end

--- Native :write checkpoints the exact live version and stays in the editor.
---@param key string
function M.save(key)
  local record = view.views[key]
  if begin(record) then checkpoint(record, function() unlock(record) end) end
end

--- Explicit confirmation flushes, checkpoints, awaits finish, then closes.
---@param key string
function M.confirm(key)
  local record = view.views[key]
  if begin(record) then checkpoint(record, function() finish(record, 'confirm') end) end
end

--- Explicit user close selects Save/Discard/Cancel before any native teardown.
---@param key string
function M.close(key)
  local record = view.views[key]
  if not begin(record) then return end
  local function choose(choice)
    if not live(record) then return end
    if choice == 'Save' then
      checkpoint(record, function() finish(record, 'normal-close') end)
    elseif choice == 'Discard' then
      -- Even when discarding, a rejected sync must not release the helper.
      if record.sync_error then
        fail(record, record.sync_error)
      else
        finish(record, 'normal-close')
      end
    else
      unlock(record)
    end
  end
  if record.text == record.saved_text and not record.sync_error then
    finish(record, 'normal-close')
  else
    hooks.prompt_started()
    vim.ui.select({ 'Save', 'Discard', 'Cancel' }, {
      prompt = 'Close commit draft: Save, Discard changes, or Cancel?',
    }, function(choice)
      hooks.prompt_finished()
      choose(choice)
    end)
  end
end

--- Abort explicitly; never infer confirmation from buffer lifetime events.
---@param key string
function M.cancel(key)
  local record = view.views[key]
  if not live(record) then return end
  if record.base and view.views[record.base] then
    view.invoke('common.cancel', { args = { runId = record.id } }, record.base)
  end
  notify(record, 'textDocument/didClose', { textDocument = { uri = record.uri } })
  view.close(record.key, false)
  view.focus(record.base)
end

--- Ambiguous API deletion/unload is cancellation, never native close intent.
---@param record table
function M.on_closed(record)
  if record.closing then return end
  notify(record, 'textDocument/didClose', { textDocument = { uri = record.uri } })
  if record.base and view.views[record.base] then
    view.invoke('common.cancel', { args = { runId = record.id } }, record.base)
  end
end

--- Recall server-owned history with the exact live base and no replacement echo.
---@param key string
---@param command string
function M.history(key, command)
  local record = view.views[key]
  if not begin(record) then return end
  local base, before = record.version, record.text
  local generation = record.generation
  local payload = params(record)
  payload.command = command
  view.request(record.buf, record.client_id, 'draft/history', payload, function(result, err)
    if not live(record) or record.disconnected or record.generation ~= generation then return end
    if err or type(result) ~= 'table' or result.type ~= 'draft/replaced'
      or result.runId ~= record.id or result.uri ~= record.uri or result.baseVersion ~= base
      or result.version ~= base + 1 or type(result.text) ~= 'string' then
      fail(record, err or 'unexpected draft history acknowledgement')
      return
    end
    record.version = result.version
    vim.bo[record.buf].modifiable = true
    if text(record) == before then
      replace(record, result.text)
    else
      -- API mutation can bypass modifiable: preserve it above the returned base.
      record.text = result.text
      record.sync_error = nil
      M.flush(record)
    end
    vim.bo[record.buf].modified = text(record) ~= record.saved_text
    unlock(record)
  end)
end

--- Display a readable sync rejection while preserving native user text.
---@param notice table
---@param client_id integer
function M.on_error(notice, client_id)
  for _, record in pairs(view.views) do
    if record.document and record.client_id == client_id and record.uri == notice.uri then
      fail(record, notice.message)
    end
  end
end

--- Retain disconnected drafts as editable recovery buffers, with no finish path.
---@param client_id integer
function M.disconnected(client_id)
  for _, record in pairs(view.views) do
    if record.document and record.client_id == client_id then
      record.disconnected = true
      fail(record, 'server disconnected; copy this draft before restarting')
      vim.bo[record.buf].modified = true
    end
  end
end

--- Place a URI-owned acwrite scratch buffer with exact UTF-8/newline bytes.
---@param record table
---@return integer
function M.place(record)
  local win = vim.api.nvim_get_current_win()
  local buf = vim.api.nvim_create_buf(true, false)
  record.buf = buf
  vim.api.nvim_buf_set_name(buf, record.uri)
  vim.bo[buf].buftype = 'acwrite'
  vim.bo[buf].bufhidden = 'hide'
  vim.bo[buf].swapfile = false
  vim.bo[buf].fixendofline = false
  replace(record, record.text)
  record.saved_text, record.saved_version = record.text, record.version
  vim.bo[buf].modified = false
  vim.api.nvim_win_set_buf(win, buf)
  return win
end

--- Register the managed-draft placement.
function M.setup()
  view.register_placement('edit', { place = M.place })
  vim.api.nvim_create_autocmd('LspProgress', {
    group = vim.api.nvim_create_augroup('magit_lsp_draft_progress', { clear = true }),
    callback = function(args)
      local data = args.data or {}
      local p = data.params or {}
      local value = p.value or {}
      if value.kind == 'begin' and value.title == 'Git' and value.message == 'commit' then
        progress[data.client_id] = p.token
      elseif value.kind == 'end' and progress[data.client_id] == p.token then
        progress[data.client_id] = nil
        -- A terminal operation cannot accept further draft barriers. This is
        -- cleanup only, never a finish inferred from progress or buffer close.
        local closed = {}
        for _, record in pairs(view.views) do
          if record.document and record.client_id == data.client_id
            and record.progress_token == p.token then closed[#closed + 1] = record end
        end
        for _, record in ipairs(closed) do view.close(record.key, false) end
      end
    end,
  })
end

local function keymap_options(record)
  local commands = {
    ['common.confirm'] = function() M.confirm(record.key) end,
    ['common.cancel'] = function() M.cancel(record.key) end,
    ['common.quit'] = function() M.close(record.key) end,
  }
  for _, command in ipairs({ 'commit-message.history-prev', 'commit-message.history-next' }) do
    commands[command] = function() M.history(record.key, command) end
  end
  return { press = function() end, command = function() end }, {
    folds = false, only_commands = true, commands = commands,
  }
end

--- Open the announced managed draft without Neovim's duplicate auto-LSP sync.
---@param effect table
---@param source table|nil
---@return table|nil
function M.open(effect, source)
  if type(effect) ~= 'table' or effect.mode ~= 'managedDraft'
    or type(effect.runId) ~= 'string' or type(effect.uri) ~= 'string'
    or not effect.uri:match('^magit%-commit://') or type(effect.text) ~= 'string'
    or effect.version ~= 1 or effect.kind ~= 'commit-message' then return nil end
  source = source and source.key and source or view.last_for_client(source and source.client_id)
  local base = source and (source.document and source.base or source.key)
  local existing = view.find(effect.runId, source and source.client_id)
  if existing then view.focus(existing.key); return existing end
  local record = view.open_document(effect, {
    placement = 'edit', filetype = 'gitcommit', client_id = source and source.client_id,
    root = source and source.root, base = base,
    keymap_options = keymap_options, on_closed = M.on_closed,
  })
  if not record then return nil end
  record.progress_token = progress[record.client_id]
  notify(record, 'textDocument/didOpen', { textDocument = {
    uri = record.uri, languageId = 'gitcommit', version = record.version, text = record.text,
  } })
  vim.api.nvim_buf_attach(record.buf, false, {
    on_lines = function()
      if record.applying or record.closing then return end
      -- on_lines runs under textlock; serialize a full snapshot on the next turn.
      hooks.schedule(function() if live(record) and not record.busy then M.flush(record) end end)
    end,
  })
  vim.api.nvim_create_autocmd('BufWriteCmd', {
    buffer = record.buf, callback = function() M.save(record.key) end,
  })
  vim.api.nvim_create_autocmd('BufUnload', {
    buffer = record.buf, callback = function()
      -- Unload is ambiguous: let an in-progress wipe/delete finish and cancel
      -- through on_closed, and cancel a plain unload once the buffer is free.
      hooks.schedule(function()
        if live(record) and not vim.bo[record.buf].loaded then M.cancel(record.key) end
      end)
    end,
  })
  vim.api.nvim_create_autocmd('BufHidden', {
    buffer = record.buf, callback = function()
      hooks.schedule(function()
        if live(record) and vim.fn.bufwinid(record.buf) == -1 then M.cancel(record.key) end
      end)
    end,
  })
  -- Command-line maps are buffer-local, not global command overrides. Native
  -- API closes and commands executed by plugins remain ambiguous and cancel.
  vim.keymap.set('c', '<CR>', function()
    if vim.fn.getcmdtype() == ':' then
      local ok, cmd = pcall(vim.api.nvim_parse_cmd, vim.fn.getcmdline(), {})
      if ok and cmd.nextcmd == '' and #cmd.args == 0
        and (cmd.cmd == 'quit' or cmd.cmd == 'close' or cmd.cmd == 'bdelete'
          or cmd.cmd == 'bwipeout' or cmd.cmd == 'wq' or cmd.cmd == 'xit') then
        hooks.schedule(function()
          if cmd.cmd == 'wq' or cmd.cmd == 'xit' then M.confirm(record.key)
          else M.close(record.key) end
        end)
        return '<C-U><C-C>'
      end
    end
    return '<CR>'
  end, { buffer = record.buf, expr = true, replace_keycodes = true })
  return record
end

return M
