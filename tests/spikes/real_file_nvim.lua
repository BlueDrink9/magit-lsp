-- Test-only native UI init. Launch with -u (not --clean, which overrides -u).
vim.opt.swapfile = false
vim.opt.shadafile = 'NONE'
vim.opt.termguicolors = true
local target = vim.env.MAGIT_REAL_FILE_TARGET
local base = vim.env.MAGIT_REAL_FILE_EVIDENCE
local client_id
vim.api.nvim_create_autocmd('BufReadPost', {
  pattern = target,
  callback = function()
    vim.bo.bufhidden = 'wipe'
    vim.bo.filetype = 'gitcommit'
    client_id = vim.lsp.start({ name = 'real-file-spike',
      cmd = { 'python3', vim.env.MAGIT_REAL_FILE_ROOT .. '/tests/spikes/real_file_lsp.py', '--record', base .. '/wire.jsonl' },
      root_dir = vim.fn.fnamemodify(target, ':h:h'),
    })
  end,
})
vim.api.nvim_create_autocmd('VimEnter', { nested = true, callback = function()
  vim.cmd.edit(vim.fn.fnameescape(target))
  vim.cmd.new()
  vim.api.nvim_buf_set_name(0, 'SpikeStatus')
  vim.cmd('wincmd p')
end })

--- Return native buffer/client state to the isolated tmux driver.
function SpikeState()
  local buf = vim.fn.bufnr(target)
  local valid = buf ~= -1 and vim.api.nvim_buf_is_valid(buf)
  local client = client_id and vim.lsp.get_client_by_id(client_id)
  return vim.json.encode({ valid = valid, current = vim.api.nvim_buf_get_name(0),
    initialized = client and client.initialized or false,
    attached = valid and client and vim.lsp.buf_is_attached(buf, client.id) or false,
    modified = valid and vim.bo[buf].modified or false,
    text = valid and table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), '\n') or '',
    language = valid and vim.bo[buf].filetype or '', error = vim.v.errmsg })
end

--- Ask the observer to exercise the client's standard showDocument handler.
function SpikeShow()
  return vim.lsp.get_client_by_id(client_id):request_sync('spike/show', {
    uri = vim.uri_from_fname(target), takeFocus = true,
  }, 3000)
end

--- Ask the observer to refresh through standard versioned workspace/applyEdit.
function SpikeRefresh()
  return vim.lsp.get_client_by_id(client_id):request_sync('spike/apply', {
    edit = { documentChanges = { { textDocument = {
      uri = vim.uri_from_fname(target), version = vim.lsp.util.buf_versions[vim.fn.bufnr(target)],
    }, edits = { { range = { start = { line = 0, character = 0 },
      ['end'] = { line = vim.api.nvim_buf_line_count(vim.fn.bufnr(target)), character = 0 } },
      newText = 'server refreshed template\n' } } } } },
  }, 3000)
end
