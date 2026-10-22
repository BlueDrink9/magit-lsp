--- Translate between the server's canonical key notation (TASKS.md §3.8) and
--- Neovim's key notation. Pure functions: no buffer or editor state.
---
--- The canonical form is `[modifier+]*key` with space-separated chord steps.
--- Neovim's form is a single `lhs` string, e.g. `ctrl+c` → `<C-c>` and
--- `g r` → `gr`.

local M = {}

--- Canonical named keys and their Neovim spellings.
local NAMED = {
  Enter = 'CR',
  Escape = 'Esc',
  Tab = 'Tab',
  Backspace = 'BS',
  Delete = 'Del',
  Insert = 'Insert',
  Home = 'Home',
  End = 'End',
  PageUp = 'PageUp',
  PageDown = 'PageDown',
  ArrowUp = 'Up',
  ArrowDown = 'Down',
  ArrowLeft = 'Left',
  ArrowRight = 'Right',
}

for index = 1, 12 do
  NAMED['F' .. index] = 'F' .. index
end

local NAMED_FROM_NVIM = {}
for canonical, nvim in pairs(NAMED) do
  NAMED_FROM_NVIM[nvim] = canonical
end

--- Canonical modifier order (§3.8) and the Neovim letter for each.
local MODIFIER_ORDER = { 'ctrl', 'alt', 'shift', 'meta' }
local MODIFIER_LETTER = { ctrl = 'C', alt = 'A', shift = 'S', meta = 'M' }
local LETTER_MODIFIER = { C = 'ctrl', A = 'alt', S = 'shift', M = 'meta' }
local WORD_MODIFIER = {
  ctrl = 'ctrl',
  control = 'ctrl',
  alt = 'alt',
  shift = 'shift',
  meta = 'meta',
}

local function fail(message)
  return nil, message
end

--- The single character of `text`, or nil when it is not one character.
local function one_char(text)
  if vim.fn.strchars(text) ~= 1 then
    return nil
  end
  return vim.fn.strcharpart(text, 0, 1)
end

local function split_ws(text)
  local out = {}
  for word in text:gmatch('%S+') do
    out[#out + 1] = word
  end
  return out
end

--- Parse one canonical step into `{ ctrl, alt, shift, meta, name|char }`.
local function parse_step(text)
  if text == '' then
    return fail('empty key')
  end
  if text == 'Plus' then
    return { key = { char = '+' } }
  end

  local parts = {}
  for part in (text .. '+'):gmatch('([^+]+)') do
    parts[#parts + 1] = part
  end
  if #parts == 0 then
    return fail('empty key')
  end

  local step = {}
  for index = 1, #parts - 1 do
    local modifier = parts[index]
    if modifier == '' then
      return fail('empty modifier in key')
    end
    local name = WORD_MODIFIER[modifier]
    if not name then
      return fail('unknown modifier: ' .. modifier)
    end
    if step[name] then
      return fail('duplicate modifier: ' .. modifier)
    end
    step[name] = true
  end

  local key = parts[#parts]
  if key == 'Space' then
    step.key = { char = ' ' }
  elseif key == 'Plus' then
    step.key = { char = '+' }
  elseif NAMED[key] then
    step.key = { name = key }
  else
    local char = one_char(key)
    if not char then
      return fail('unknown key: ' .. key)
    end
    if char == '+' then
      return fail('write the plus key as `Plus`')
    end
    if char:match('%s') then
      return fail('write the space key as `Space`')
    end
    if step.shift and not (step.ctrl or step.alt or step.meta) then
      return fail('`shift+' .. char .. '` is invalid; write the shifted character instead')
    end
    step.key = { char = char }
  end

  return step
end

--- The Neovim spelling of one parsed step.
local function step_to_lhs(step)
  local letters = {}
  for _, modifier in ipairs(MODIFIER_ORDER) do
    if step[modifier] then
      letters[#letters + 1] = MODIFIER_LETTER[modifier]
    end
  end

  local key = step.key
  local name
  if key.name then
    name = NAMED[key.name]
    if not name then
      return fail('unknown key: ' .. key.name)
    end
  elseif key.char == ' ' then
    name = 'Space'
  elseif key.char == '+' then
    name = '+'
  elseif key.char == '<' then
    name = 'lt'
  else
    name = key.char
  end

  if #letters == 0 then
    if key.name then
      return '<' .. name .. '>'
    end
    if key.char == ' ' then
      return '<Space>'
    end
    if key.char == '<' then
      return '<lt>'
    end
    return key.char
  end

  return '<' .. table.concat(letters, '-') .. '-' .. name .. '>'
end

--- Split a canonical chord into its step strings, or nil when it is invalid.
---@param chord string
---@return string[]|nil
function M.steps(chord)
  if type(chord) ~= 'string' then
    return nil
  end
  local steps = split_ws(chord)
  if #steps == 0 then
    return nil
  end
  for _, step in ipairs(steps) do
    local parsed, err = parse_step(step)
    if not parsed then
      return nil, err
    end
  end
  return steps
end

--- Translate a canonical chord to one Neovim `lhs`.
---@param chord string
---@return string|nil
function M.to_lhs(chord)
  local steps, err = M.steps(chord)
  if not steps then
    return nil, err
  end
  local out = {}
  for _, text in ipairs(steps) do
    local step = parse_step(text)
    local lhs, step_err = step_to_lhs(step)
    if not lhs then
      return nil, step_err
    end
    out[#out + 1] = lhs
  end
  return table.concat(out)
end

--- Turn a Neovim special-key body such as `C-S-c` or `CR` into canonical.
local function special_to_canonical(special)
  local parts = {}
  for part in (special .. '-'):gmatch('([^-]*)-') do
    parts[#parts + 1] = part
  end
  if #parts == 0 then
    return fail('empty key')
  end

  local key = parts[#parts]
  local modifiers = {}
  for index = 1, #parts - 1 do
    local part = parts[index]
    local modifier = LETTER_MODIFIER[part] or WORD_MODIFIER[part:lower()]
    if not modifier then
      return fail('unknown modifier: ' .. part)
    end
    if modifiers[modifier] then
      return fail('duplicate modifier: ' .. part)
    end
    modifiers[modifier] = true
  end

  local canonical_key
  if key == 'lt' then
    canonical_key = '<'
  elseif key == 'Space' then
    canonical_key = 'Space'
  elseif key == 'Plus' then
    canonical_key = 'Plus'
  elseif NAMED_FROM_NVIM[key] then
    canonical_key = NAMED_FROM_NVIM[key]
  else
    local char = one_char(key)
    if not char then
      return fail('unknown key: ' .. key)
    end
    if char == '+' then
      canonical_key = 'Plus'
    elseif char:match('%s') then
      canonical_key = 'Space'
    else
      canonical_key = char
    end
  end

  if type(canonical_key) ~= 'string' then
    return nil, canonical_key
  end

  if canonical_key == '<' or canonical_key == 'Space' or canonical_key == 'Plus' or NAMED[canonical_key] then
    -- Named keys and the two spelled-out characters.
  elseif modifiers.shift and not (modifiers.ctrl or modifiers.alt or modifiers.meta) then
    return fail('shifted character keys have no `shift+` form')
  end

  local out = {}
  for _, modifier in ipairs(MODIFIER_ORDER) do
    if modifiers[modifier] then
      out[#out + 1] = modifier
    end
  end
  out[#out + 1] = canonical_key
  return table.concat(out, '+')
end

--- Translate one Neovim `lhs` to a canonical chord.
---@param lhs string
---@return string|nil
function M.from_lhs(lhs)
  if type(lhs) ~= 'string' then
    return nil, 'lhs must be a string'
  end
  if lhs == '' then
    return nil, 'empty lhs'
  end

  local steps = {}
  local index = 1
  while index <= #lhs do
    local char = lhs:sub(index, index)
    if char == '<' then
      local close = lhs:find('>', index + 1, true)
      if not close then
        return nil, 'unterminated `<` in lhs'
      end
      local canonical, err = special_to_canonical(lhs:sub(index + 1, close - 1))
      if not canonical then
        return nil, err
      end
      steps[#steps + 1] = canonical
      index = close + 1
    elseif char == ' ' then
      steps[#steps + 1] = 'Space'
      index = index + 1
    elseif char == '+' then
      steps[#steps + 1] = 'Plus'
      index = index + 1
    else
      steps[#steps + 1] = char
      index = index + 1
    end
  end

  return table.concat(steps, ' ')
end

--- Termcodes for `nvim_feedkeys` with `escape_ks = false`.
---@param lhs string
---@return string
function M.termcodes(lhs)
  return vim.api.nvim_replace_termcodes(lhs, true, false, true)
end

return M
