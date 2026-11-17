-- Plugin-free child Neovim used by the capability-preset acceptance runner.
vim.opt.swapfile = false
vim.opt.shadafile = 'NONE'
local repo, server = assert(vim.env.PLAIN_REPO), assert(vim.env.PLAIN_SERVER)
local presets = {
  minimal = { workspace = { applyEdit = true }, textDocument = { codeAction = {} } },
  showDocument = { workspace = { applyEdit = true }, textDocument = { codeAction = {} }, window = { showDocument = { support = true } } },
  completion = { workspace = { applyEdit = true }, textDocument = { codeAction = {}, completion = { completionItem = { snippetSupport = true } } } },
  progress = { workspace = { applyEdit = true }, textDocument = { codeAction = {} }, window = { workDoneProgress = true } },
  full = { workspace = { applyEdit = true, workspaceEdit = { documentChanges = true } }, textDocument = { codeAction = {}, completion = { completionItem = { snippetSupport = true } } }, window = { showDocument = { support = true }, workDoneProgress = true } },
}
local preset = assert(presets[vim.env.PLAIN_PRESET])
local evidence = { shows = {}, messages = {}, progress = {}, creates = {} }
for _, method in ipairs({ 'window/showDocument', 'window/showMessage', 'window/workDoneProgress/create', '$/progress' }) do
  local native = assert(vim.lsp.handlers[method], method)
  vim.lsp.handlers[method] = function(err, params, ctx, config)
    local key = ({ ['window/showDocument'] = 'shows', ['window/showMessage'] = 'messages',
      ['window/workDoneProgress/create'] = 'creates', ['$/progress'] = 'progress' })[method]
    table.insert(evidence[key], vim.deepcopy(params))
    return native(err, params, ctx, config)
  end
end
local function wait(label, predicate)
  assert(vim.wait(10000, predicate, 10), 'timeout: ' .. label)
end
local function text(buf)
  return table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), '\n')
end
local function exercise(client, buf)
  wait('status render', function() return text(buf):find('tracked.txt', 1, true) ~= nil end)
  local uri = vim.uri_from_bufnr(buf)
  local initial = text(buf)
  local function request(method, params, target)
    local response = assert(client:request_sync(method, params, 10000, target or buf), method)
    assert(not response.err, vim.inspect(response.err))
    return response.result
  end
  local function actions(target)
    local row = 0
    for index, line in ipairs(vim.api.nvim_buf_get_lines(target, 0, -1, false)) do
      if line:find(target == buf and (text(buf):find('Commit', 1, true) and 'Commit' or 'tracked.txt') or 'Native draft edit', 1, true) then
        row = index - 1; break
      end
    end
    return request('textDocument/codeAction', { textDocument = { uri = vim.uri_from_bufnr(target) },
      range = { start = { line = row, character = 0 }, ['end'] = { line = row, character = 0 } },
      context = { diagnostics = {} } }, target) or {}
  end
  local function find_action(target, command, item)
    for _, action in ipairs(actions(target)) do
      local arg = action.command and action.command.arguments[1]
      if arg and arg.command == command and (not item or arg.target.itemKey == item) then return action.command end
    end
    error('missing action: ' .. command .. ' ' .. tostring(item))
  end
  request('workspace/executeCommand', find_action(buf, 'open.commit'))
  local completed, outcome = false, nil
  assert(client:request('workspace/executeCommand', find_action(buf, 'menu.run-action', 'c'), function(err, result)
    assert(not err, vim.inspect(err)); outcome = result; evidence.outcome = result; completed = true
  end, buf))
  local draft = repo .. '/.git/COMMIT_EDITMSG'
  wait('Git editor draft', function() return vim.fn.filereadable(draft) == 1 end)
  if preset.window and preset.window.showDocument then
    wait('native showDocument', function() return vim.api.nvim_buf_get_name(0) == draft end)
    assert(#evidence.shows == 1 and evidence.shows[1].uri == vim.uri_from_fname(draft))
  else
    wait('manual-open fallback', function()
      for _, message in ipairs(evidence.messages) do
        if message.message:find(draft, 1, true) and message.message:find('Open', 1, true) then return true end
      end
      return false
    end)
    vim.cmd.edit(vim.fn.fnameescape(draft))
    vim.lsp.buf_attach_client(0, client.id)
  end
  local draft_buf = vim.api.nvim_get_current_buf()
  assert(vim.lsp.buf_attach_client(draft_buf, client.id), 'attach native draft buffer')
  local template = table.concat(vim.fn.readfile(draft), '\n')
  assert(text(draft_buf) == template and template:find('tracked.txt', 1, true), 'native draft matches Git template')
  local opened = find_action(draft_buf, 'common.cancel').arguments[1]
  assert(opened.uri == vim.uri_from_bufnr(draft_buf), 'server owns attached draft URI')
  assert(opened.version == vim.lsp.util.buf_versions[draft_buf], 'native didOpen version retained by server')
  vim.api.nvim_buf_set_lines(draft_buf, 0, 0, false, { 'Native draft edit' })
  local edited = text(draft_buf)
  -- The subsequent request flushes native didChange; Abort must address that version.
  local offered = actions(draft_buf)
  assert(#offered > 0, 'draft has synchronized actions')
  for _, action in ipairs(offered) do
    local synchronized = action.command.arguments[1]
    assert(synchronized.uri == opened.uri and synchronized.version > opened.version,
      'native didChange advances server draft version')
    assert(synchronized.version == vim.lsp.util.buf_versions[draft_buf], 'server has exact native edited version')
  end
  if preset.textDocument.completion then
    assert(not client.server_capabilities.completionProvider, 'completion is not implemented by this server')
    evidence.completion = 'declared; server does not advertise completionProvider'
  end
  if preset.window and preset.window.workDoneProgress then
    local token
    wait('native progress begin', function()
      for _, event in ipairs(evidence.progress) do
        if event.value.kind == 'begin' then token = event.token; return true end
      end
      return false
    end)
    assert(#evidence.creates == 1 and evidence.creates[1].token == token)
    assert(client:notify('window/workDoneProgress/cancel', { token = token }))
    wait('native progress end', function()
      for _, event in ipairs(evidence.progress) do
        if event.token == token and event.value.kind == 'end' then return true end
      end
      return false
    end)
  else
    local result = request('workspace/executeCommand', find_action(draft_buf, 'common.cancel'), draft_buf)
    assert(result.type == 'aborted', 'document Abort acknowledged')
    assert(#evidence.progress == 0 and #evidence.creates == 0)
  end
  wait('draft operation response', function() return completed end)
  wait('caller restored', function() return not text(buf):find('COMMIT_EDITMSG', 1, true) end)
  assert(text(buf) == initial, 'cancel restores status document')
  assert(text(draft_buf) == edited, 'cancel preserves unsaved native user edit')
  assert(#actions(draft_buf) == 0, 'cancel detaches draft actions')
  evidence.uri, evidence.text, evidence.draft_text = uri, text(buf), edited
  evidence.client_version = vim.lsp.util.buf_versions[buf]
  evidence.project_modules = {}
  for key in pairs(package.loaded) do
    if tostring(key):match('magit%-lsp') then table.insert(evidence.project_modules, key) end
  end
  local stopped = false
  client:stop()
  wait('native client shutdown', function() stopped = client:is_stopped(); return stopped end)
  evidence.stopped = stopped
end
vim.api.nvim_create_autocmd('VimEnter', { once = true, callback = function()
  vim.api.nvim_buf_set_name(0, repo .. '/.magit')
  local buf = vim.api.nvim_get_current_buf()
  local client = vim.lsp.start({ name = 'magit-lsp-capability-preset', cmd = { server }, root_dir = repo, capabilities = preset,
    before_init = function(params)
      params.capabilities = vim.deepcopy(preset)
      evidence.capabilities = vim.deepcopy(params.capabilities)
    end })
  assert(client, 'vim.lsp.start failed')
  vim.lsp.buf_attach_client(0, client)
  vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'initial' })
  vim.api.nvim_buf_call(0, function() vim.api.nvim_exec_autocmds('BufReadPost', { modeline = false }) end)
  vim.lsp.buf_attach_client(0, client)
  vim.schedule(function()
    local ok, err = xpcall(function() exercise(assert(vim.lsp.get_client_by_id(client)), buf) end, debug.traceback)
    if not ok then evidence.error = err; vim.lsp.stop_client(client, true) end
    local f = assert(io.open(vim.env.PLAIN_RESULT, 'w')); f:write(vim.json.encode(evidence)); f:close()
    vim.cmd('qa!')
  end)
end })
