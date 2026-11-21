--- Dedicated initializer for the interactive launcher (`make neovim`).
---
--- It loads only this checkout: the client runtime from `clients/neovim`, its
--- plugin entry point and the `:Magit` command. It deliberately does not use a
--- user configuration, a plugin manager or the MiniTest test initializer, and
--- it opens the repository named by `$MAGIT_LSP_LAUNCH_REPO`.

local script_dir = vim.fn.fnamemodify(vim.fn.expand("<sfile>:p"), ":h")
local project_root = vim.fn.fnamemodify(script_dir, ":h")
local client_runtime = project_root .. "/clients/neovim"

vim.opt.runtimepath:prepend(client_runtime)
vim.opt.packpath = {}
vim.cmd.source(client_runtime .. "/plugin/magit-lsp.lua")

-- The launcher creates these directories and removes them when Neovim exits.
local launch_dir = vim.env.MAGIT_LSP_LAUNCH_DIR
if launch_dir and launch_dir ~= "" then
  vim.opt.directory = { launch_dir .. "/swap" }
  vim.opt.backupdir = { launch_dir .. "/backup" }
  vim.opt.undodir = { launch_dir .. "/undo" }
  vim.opt.viewdir = launch_dir .. "/view"
end

local repo = vim.env.MAGIT_LSP_LAUNCH_REPO
require("magit-lsp").open(repo ~= nil and repo ~= "" and repo or nil)
