--- Register the Neovim entry command when Neovim loads plugins.
---
--- The command only forwards to the public `magit-lsp.open` path, so loading
--- this file never starts a session, reads configuration or calls `setup()`.

if vim.g.loaded_magit_lsp then
  return
end
vim.g.loaded_magit_lsp = true

vim.api.nvim_create_user_command("Magit", function(opts)
  require("magit-lsp").open(opts.args ~= "" and opts.args or nil)
end, {
  nargs = "?",
  complete = "dir",
  desc = "Open the magit-lsp status view",
})
