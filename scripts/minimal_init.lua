local script_dir = vim.fn.fnamemodify(vim.fn.expand('<sfile>:p'), ':h')
local project_root = vim.fn.fnamemodify(script_dir, ':h')

local function add_path(path)
  if vim.fn.isdirectory(path) == 1 then
    vim.opt.rtp:append(path)
  end
end

add_path(project_root)

local mini_path = project_root .. '/build/deps/mini.nvim'
if vim.fn.isdirectory(mini_path) ~= 1 then
  vim.fn.mkdir(project_root .. '/build/deps', 'p')
  vim.cmd('echo "Installing `mini.nvim`" | redraw')
  vim.fn.system({
    'git', 'clone', '--filter=blob:none', '--depth', '1',
    'https://github.com/echasnovski/mini.nvim', mini_path,
  })
end
add_path(mini_path)

if #vim.api.nvim_list_uis() == 0 then
  require('mini.test').setup()
end

-- The UI harness (`scripts/ui`) starts Neovim with these variables. Tests that
-- set up the client themselves leave them unset. `MAGIT_LSP_TRACE` is read by
-- the debug module directly.
local ui_root = vim.env.MAGIT_LSP_UI_ROOT
if ui_root and ui_root ~= '' then
  local profile = vim.env.MAGIT_LSP_PROFILE
  require('magit-lsp').setup({
    root_dir = ui_root,
    profile = profile ~= '' and profile or nil,
  })
end
