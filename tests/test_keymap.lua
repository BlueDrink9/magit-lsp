local MiniTest = require('mini.test')
local keys = require('magit-lsp.keys')

local T = MiniTest.new_set()

--- The translator table of TASKS.md §3.8, both directions.
local CASES = {
  { 's', 's' },
  { 'S', 'S' },
  { '?', '?' },
  { 'Enter', '<CR>' },
  { 'Escape', '<Esc>' },
  { 'Tab', '<Tab>' },
  { 'shift+Tab', '<S-Tab>' },
  { 'Delete', '<Del>' },
  { 'Backspace', '<BS>' },
  { 'Insert', '<Insert>' },
  { 'Home', '<Home>' },
  { 'End', '<End>' },
  { 'PageUp', '<PageUp>' },
  { 'PageDown', '<PageDown>' },
  { 'ArrowUp', '<Up>' },
  { 'ArrowDown', '<Down>' },
  { 'ArrowLeft', '<Left>' },
  { 'ArrowRight', '<Right>' },
  { 'ctrl+c', '<C-c>' },
  { 'alt+ArrowDown', '<A-Down>' },
  { 'ctrl+Enter', '<C-CR>' },
  { 'ctrl+shift+c', '<C-S-c>' },
  { 'Space', '<Space>' },
  { 'Plus', '+' },
  { 'F1', '<F1>' },
  { 'F5', '<F5>' },
  { 'F12', '<F12>' },
  { 'g g', 'gg' },
  { 'z a', 'za' },
  { '] ]', ']]' },
  { 'ctrl+k ctrl+s', '<C-k><C-s>' },
}

T['canonical keys translate to Neovim lhs'] = function()
  for _, case in ipairs(CASES) do
    local canonical, lhs = case[1], case[2]
    MiniTest.expect.equality(keys.to_lhs(canonical), lhs, canonical)
  end
end

T['Neovim lhs translate back to canonical keys'] = function()
  for _, case in ipairs(CASES) do
    local canonical, lhs = case[1], case[2]
    MiniTest.expect.equality(keys.from_lhs(lhs), canonical, lhs)
  end
end

T['translation round-trips both ways'] = function()
  for _, case in ipairs(CASES) do
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
  MiniTest.expect.equality(keys.steps('shift+s'), nil)
end

T['invalid canonical keys are rejected'] = function()
  for _, canonical in ipairs({
    '',
    '   ',
    'shift+s',
    'shift+S',
    'ctrl+',
    '+',
    'ctrl+ctrl+s',
    'ctrl+nope',
    'NotAKey',
    'ctrl+space',
  }) do
    MiniTest.expect.equality(keys.to_lhs(canonical), nil, canonical)
  end
end

T['invalid Neovim lhs are rejected'] = function()
  for _, lhs in ipairs({
    '',
    '<C->',
    '<C-nope>',
    '<nope>',
    '<S-s>',
    'abc<',
  }) do
    MiniTest.expect.equality(keys.from_lhs(lhs), nil, lhs)
  end
end

return T
