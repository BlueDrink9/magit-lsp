--- Native COMMIT_EDITMSG editing with bounded full sync and semantic confirmation.
local view = require('magit-lsp.view')
local hooks = require('magit-lsp.hooks')
local M = {}
local progress = {}
-- Remember announcements after terminal teardown; file URIs can be reused.
local seen = {}
local attach

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
      or result.version ~= (payload.version or payload.arguments[1].version) or record.version ~= base then
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
  local payload = params(record, reason == 'confirm' and record.version or record.saved_version)
  payload.command = 'common.confirm'
  payload.source = reason == 'confirm' and 'synchronized' or 'saved-file'
  barrier(record, 'workspace/executeCommand', { command = 'magit.command', arguments = { payload } }, 'confirmed', function()
    view.close(record.key, false)
    view.focus(record.base)
  end)
end

local function checkpoint(record, done)
  local value = record.text
  local ok, err = pcall(vim.api.nvim_buf_call, record.buf, function() vim.cmd('silent write') end)
  if not ok then fail(record, err); return end
  record.saved_version = record.version
  record.saved_text = value
  record.sync_error = nil
  vim.bo[record.buf].modified = text(record) ~= value
  done()
end

local function begin(record)
  if not live(record) or record.busy or record.disconnected then return false end
  if not M.flush(record) then return false end
  record.busy = true
  vim.bo[record.buf].modifiable = false
  return true
end

--- Native :write saves disk text and stays in the editor.
---@param key string
function M.save(key)
  local record = view.views[key]
  if begin(record) then checkpoint(record, function() unlock(record) end) end
end

--- Explicit confirmation flushes the exact version, awaits safe writeback, then closes.
---@param key string
function M.confirm(key)
  local record = view.views[key]
  if begin(record) then finish(record, 'confirm') end
end

--- Abort explicitly without sending a close that might race as confirmation.
---@param key string
function M.cancel(key)
  local record = view.views[key]
  if not live(record) then return end
  if record.base and view.views[record.base] then
    view.invoke('common.cancel', { args = { runId = record.id } }, record.base)
  end
  view.close(record.key, false)
  view.focus(record.base)
end

--- Ordinary buffer teardown confirms saved disk text by the accepted LSP assumption.
---@param record table
function M.on_closed(record)
  if record.closing or record.disconnected or record.detached or vim.v.exiting ~= vim.NIL then return end
  -- Capture edits even when native teardown precedes the scheduled on_lines
  -- flush. Saved-file confirmation still reads disk, never this snapshot.
  M.flush(record)
  record.detached = true
  notify(record, 'textDocument/didClose', { textDocument = { uri = record.uri } })
end

--- Retain disconnected drafts as editable recovery buffers, with no finish path.
---@param client_id integer
function M.disconnected(client_id)
  for _, record in pairs(view.views) do
    if record.document and record.client_id == client_id then
      record.disconnected = true
      fail(record, 'server disconnected; copy this draft before restarting')
      if vim.api.nvim_buf_is_valid(record.buf) then vim.bo[record.buf].modified = true end
    end
  end
end

--- Place the real native file buffer with exact UTF-8/newline bytes.
---@param record table
---@return integer
function M.place(record)
  local win = vim.api.nvim_get_current_win()
  record.previous_buf = vim.api.nvim_win_get_buf(win)
  local buf = vim.fn.bufadd(vim.uri_to_fname(record.uri))
  vim.fn.bufload(buf)
  record.buf = buf
  vim.bo[buf].buftype = ''
  vim.bo[buf].bufhidden = 'wipe'
  vim.bo[buf].swapfile = false
  vim.bo[buf].fixendofline = false
  replace(record, record.text)
  record.saved_text, record.saved_version = record.text, record.version
  vim.bo[buf].modified = false
  vim.api.nvim_win_set_buf(win, buf)
  return win
end

--- Register the real commit-file placement.
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
    ['common.quit'] = function() vim.cmd.bdelete() end,
  }
  return { press = function() end, command = function() end }, {
    folds = false, only_commands = true, commands = commands,
  }
end

--- Open the announced native file with one explicit full-sync stream.
---@param effect table
---@param source table|nil
---@return table|nil
function M.open(effect, source)
  if type(effect) ~= 'table' or effect.mode ~= 'realFile'
    or type(effect.runId) ~= 'string' or type(effect.uri) ~= 'string'
    or not effect.uri:match('^file://.*/COMMIT_EDITMSG$') or type(effect.text) ~= 'string'
    or type(effect.version) ~= 'number' or effect.version < 1
    or effect.version % 1 ~= 0 or effect.kind ~= 'commit-message' then return nil end
  source = source and source.key and source or view.last_for_client(source and source.client_id)
  local base = source and (source.document and source.base or source.key)
  local existing = view.find(effect.runId, source and source.client_id)
  if existing then
    if existing.uri ~= effect.uri or existing.closing or existing.disconnected then return nil end
    if existing.detached and not vim.api.nvim_buf_is_valid(existing.buf) then
      local value, saved, version = existing.text, existing.saved_text, existing.saved_version
      existing.win = M.place(existing)
      existing.saved_text, existing.saved_version = saved, version
      replace(existing, value)
      vim.bo[existing.buf].filetype = 'gitcommit'
      vim.bo[existing.buf].modified = true
      existing.detached = false
      existing.generation = (existing.generation or 0) + 1
      existing.busy = false
      existing.sync_error = 'saved-file close failed'
      attach(existing)
      view.apply_keymap(existing.keymap, existing.client_id)
    end
    view.focus(existing.key)
    return existing
  end
  local identity = tostring(source and source.client_id) .. ':' .. effect.runId
  if seen[identity] or effect.version ~= 1 then return nil end
  for _, record in pairs(view.views) do
    if record.document and record.uri == effect.uri and record.client_id == (source and source.client_id) then
      return nil
    end
  end
  local record = view.open_document(effect, {
    placement = 'edit', filetype = 'gitcommit', client_id = source and source.client_id,
    root = source and source.root, base = base,
    keymap_options = keymap_options, on_closed = M.on_closed,
  })
  if not record then return nil end
  seen[identity] = true
  record.progress_token = progress[record.client_id]
  notify(record, 'textDocument/didOpen', { textDocument = {
    uri = record.uri, languageId = 'gitcommit', version = record.version, text = record.text,
  } })
  attach(record)
  return record
end

attach = function(record)
  vim.api.nvim_buf_attach(record.buf, false, {
    on_lines = function()
      if record.applying or record.closing then return end
      -- on_lines runs under textlock; serialize a full snapshot on the next turn.
      hooks.schedule(function() if live(record) and not record.busy then M.flush(record) end end)
    end,
  })
  vim.api.nvim_create_autocmd('BufWritePost', {
    buffer = record.buf, callback = function()
      if not record.busy and not M.flush(record) then return end
      record.saved_text, record.saved_version = record.text, record.version
      record.sync_error = nil
    end,
  })
  vim.api.nvim_create_autocmd('BufUnload', {
    buffer = record.buf, callback = function()
      -- Native teardown confirms disk bytes, except during editor shutdown.
      if not record.closing and not record.disconnected then M.on_closed(record) end
    end,
  })
end

return M
