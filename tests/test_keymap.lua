local MiniTest = require('mini.test')
local keys = require('magit-lsp.keys')

local T = MiniTest.new_set()

--- The shared §3.8 fixture; the `src/keys.rs` unit tests pin the Rust
--- translator to the same cases.
local here = vim.fs.dirname(debug.getinfo(1, 'S').source:sub(2))
local fixture_file = vim.fs.joinpath(here, 'data', 'keys.json')
local fixture_file_handle = io.open(fixture_file, 'r')
if not fixture_file_handle then
  error('cannot read the shared keys fixture: ' .. fixture_file)
end
local FIXTURE = vim.json.decode(fixture_file_handle:read('a'))
fixture_file_handle:close()

T['canonical keys translate to Neovim lhs'] = function()
  for _, case in ipairs(FIXTURE.toNeovim) do
    local canonical, lhs = case[1], case[2]
    MiniTest.expect.equality(keys.to_lhs(canonical), lhs, canonical)
  end
end

T['Neovim lhs translate back to canonical keys'] = function()
  for _, case in ipairs(FIXTURE.toNeovim) do
    local canonical, lhs = case[1], case[2]
    MiniTest.expect.equality(keys.from_lhs(lhs), canonical, lhs)
  end
end

T['translation round-trips both ways'] = function()
  for _, case in ipairs(FIXTURE.toNeovim) do
    local canonical, lhs = case[1], case[2]
    MiniTest.expect.equality(keys.to_lhs(keys.from_lhs(lhs)), lhs, lhs)
    MiniTest.expect.equality(keys.from_lhs(keys.to_lhs(canonical)), canonical, canonical)
  end
end

T['chord steps are exposed for the server'] = function()
  MiniTest.expect.equality(keys.steps('g r'), { 'g', 'r' })
  MiniTest.expect.equality(keys.steps('ctrl+k ctrl+s'), { 'ctrl+k', 'ctrl+s' })
  MiniTest.expect.equality(keys.steps('s'), { 's' })
  MiniTest.expect.equality(keys.steps(''), nil)
end

T['invalid Neovim lhs are rejected'] = function()
  for _, lhs in ipairs(FIXTURE.invalidNeovim) do
    MiniTest.expect.equality(keys.from_lhs(lhs), nil, lhs)
  end
end

return T
