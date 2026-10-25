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
local trace_path = vim.env.MAGIT_LSP_TRACE

-- The debug recorder backs the harness `idle()`/snapshot contract and the
-- trace. Product code only requires `magit-lsp.hooks`, so loading it here is
-- what opts a test or harness run in; a normal `setup()` never loads it.
if (trace_path and trace_path ~= '') or (ui_root and ui_root ~= '') or rawget(_G, 'MiniTest') then
  require('magit-lsp.debug')
end

if ui_root and ui_root ~= '' then
  local profile = vim.env.MAGIT_LSP_PROFILE
  -- The status placement is a client-only option; scenarios use this hook to
  -- run with the default (`replace`) or an explicit split/tab placement.
  local placement = vim.env.MAGIT_LSP_PLACEMENT
  require('magit-lsp').setup({
    root_dir = ui_root,
    profile = profile ~= '' and profile or nil,
    status = placement and placement ~= '' and { placement = placement } or nil,
  })
end
