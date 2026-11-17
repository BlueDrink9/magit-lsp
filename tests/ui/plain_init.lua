-- Standalone standard-LSP init for the Plain native smoke. No project Lua modules.
vim.opt.swapfile = false
vim.opt.undofile = false
vim.opt.hidden = true
vim.opt.shortmess:append('I')
vim.g.plain_smoke_rich_notifications = 0
vim.lsp.log.set_level('trace')
if vim.env.MAGIT_LSP_UI_PROBE_FILE then
  local probe_path = vim.env.MAGIT_LSP_UI_PROBE_FILE
  local function write_probe(record)
    _G.plain_probe_sequence = (_G.plain_probe_sequence or 0) + 1
    record.sequence = _G.plain_probe_sequence
    record.time_ns = vim.uv.hrtime()
    local file = assert(io.open(probe_path, 'a'))
    file:write(vim.json.encode(record), '\n')
    file:close()
  end
  local function diagnostic_buffer(buf)
    local name = vim.api.nvim_buf_get_name(buf or vim.api.nvim_get_current_buf())
    return name:match('/%.magit$') or name:match('/COMMIT_EDITMSG$')
  end
  local original_print = print
  _G.print = function(...)
    local message = table.concat(vim.tbl_map(tostring, { ... }), '\t')
    local buf = vim.api.nvim_get_current_buf()
    if diagnostic_buffer(buf) and message:find('Buffer .* newer than edits%.') then
      write_probe({ event = 'warning-print-before', message = message,
        traceback = debug.traceback(), mode = vim.api.nvim_get_mode(), cmdtype = vim.fn.getcmdtype() })
    end
    local result = original_print(...)
    if diagnostic_buffer(buf) and message:find('Buffer .* newer than edits%.') then
      write_probe({ event = 'warning-print-after', message = message,
        mode = vim.api.nvim_get_mode(), cmdtype = vim.fn.getcmdtype() })
    end
    return result
  end
  local original_notify = vim.notify
  vim.notify = function(message, ...)
    local buf = vim.api.nvim_get_current_buf()
    local scoped = diagnostic_buffer(buf) and tostring(message):find('Buffer .* newer than edits%.')
    if scoped then write_probe({ event = 'warning-notify-before', message = tostring(message),
      traceback = debug.traceback(), mode = vim.api.nvim_get_mode(), cmdtype = vim.fn.getcmdtype() }) end
    local result = original_notify(message, ...)
    if scoped then write_probe({ event = 'warning-notify-after', message = tostring(message),
      mode = vim.api.nvim_get_mode(), cmdtype = vim.fn.getcmdtype() }) end
    return result
  end
  local function probe(event, key)
    local buf = vim.api.nvim_get_current_buf()
    local name = vim.api.nvim_buf_get_name(buf)
    if not name:match('/COMMIT_EDITMSG$') then return end
    local client = vim.lsp.get_client_by_id(vim.g.plain_smoke_client_id or -1)
    local autocmds = {}
    for _, event in ipairs({ 'InsertEnter', 'InsertLeave', 'TextChanged', 'TextChangedI', 'BufWritePre', 'BufWritePost' }) do
      for _, autocmd in ipairs(vim.api.nvim_get_autocmds({ event = event })) do
        autocmds[#autocmds + 1] = { event = event, desc = autocmd.desc, command = autocmd.command }
      end
    end
    local cursor = vim.api.nvim_win_get_cursor(0)
    local messages = vim.fn.execute('messages')
    local mode_state = vim.api.nvim_get_mode()
    local record = {
      time_ns = vim.uv.hrtime(), event = event, key = key,
      keytrans = key and vim.fn.keytrans(key) or nil,
      text = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), '\n'),
      mode = mode_state.mode, blocking = mode_state.blocking,
      cursor = { row = cursor[1] - 1, col = cursor[2] },
      cmdtype = vim.fn.getcmdtype(), messages = messages,
      changedtick = vim.api.nvim_buf_get_changedtick(buf),
      uri = vim.uri_from_bufnr(buf), version = client and vim.lsp.util.buf_versions[buf],
      insert_enter = vim.fn.maparg('<CR>', 'i', false, true),
      normal_enter = vim.fn.maparg('<CR>', 'n', false, true),
      paste = vim.o.paste, formatoptions = vim.bo[buf].formatoptions,
      autocmds = autocmds,
    }
    record.sequence = nil
    write_probe(record)
  end
  vim.on_key(function(key) if key == '\r' or key == '\n' then probe('native-key', key) end end)
  vim.api.nvim_create_autocmd({ 'TextChanged', 'TextChangedI', 'InsertEnter', 'InsertLeave', 'ModeChanged' }, {
    callback = function(event) probe(event.event) end,
  })
end
vim.lsp.handlers['view/render'] = function()
  vim.g.plain_smoke_rich_notifications = vim.g.plain_smoke_rich_notifications + 1
end

local repo = vim.env.PLAIN_REPO
local server = vim.env.PLAIN_SERVER
local server_wrapper = vim.env.MAGIT_LSP_SERVER_WRAPPER
if server_wrapper then server = server_wrapper end
assert(repo and server)

vim.api.nvim_create_autocmd('VimEnter', {
  once = true,
  callback = function()
    local capabilities = vim.lsp.protocol.make_client_capabilities()
    capabilities.window.showDocument = { support = vim.env.MAGIT_LSP_UI_CLIENT == 'plain' }
    local client_id = vim.lsp.start({
      name = 'magit-lsp-plain-smoke',
      cmd = { server },
      root_dir = repo,
      capabilities = capabilities,
    })
    assert(client_id, 'vim.lsp.start did not create a client')
    vim.g.plain_smoke_client_id = client_id
    vim.lsp.buf_attach_client(0, client_id)
    vim.api.nvim_create_autocmd('BufReadPost', {
      callback = function(event)
        vim.lsp.buf_attach_client(event.buf, client_id)
      end,
    })
  end,
})

-- The standalone smoke deliberately keeps its original declarations and setup.
if vim.env.MAGIT_LSP_UI_CLIENT == 'plain' then
  dofile(vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':h') .. '/plain_scenario.lua')
end
