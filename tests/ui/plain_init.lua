-- Standalone standard-LSP init for the Plain native smoke. No project Lua modules.
vim.opt.swapfile = false
vim.opt.undofile = false
vim.opt.hidden = true
vim.opt.shortmess:append('I')
vim.g.plain_smoke_rich_notifications = 0
vim.lsp.log.set_level('trace')
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
    capabilities.window.showDocument = nil
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
