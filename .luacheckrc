-- Neovim's Lua runtime is the supported standard for this client.
std = 'lua51'
globals = { 'vim' }

-- Lua source and the Neovim test bootstrap are the checked code.
include = { 'lua/**/*.lua', 'scripts/**/*.lua', 'tests/**/*.lua' }
