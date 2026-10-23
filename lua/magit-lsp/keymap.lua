--- Apply a server keymap to a Neovim buffer, atomically (§3.9).
---
--- Neovim has no named buffer-local maps, so the applied set is tracked in
--- `M.applied[bufnr]`. Applying a keymap deletes exactly that set, sets the new
--- one and records it, all in one synchronous call: no key can be pressed
--- between the delete and the set.

local keys = require('magit-lsp.keys')

local M = {}

--- The tracked buffer-local maps, keyed by buffer number.
---@type table<integer, table[]>
M.applied = {}

--- Native fold keys and the server command each mirrors (§3.9).
local FOLDS = {
  za = { command = 'common.toggle-section' },
  zA = { command = 'common.toggle-section', args = { recursive = true } },
  zo = { command = 'common.show-section' },
  zO = { command = 'common.show-section', args = { recursive = true } },
  zc = { command = 'common.hide-section' },
  zC = { command = 'common.hide-section', args = { recursive = true } },
  zR = { command = 'common.show-all' },
  zM = { command = 'common.hide-all' },
  zr = { command = 'common.more-detail' },
  zm = { command = 'common.less-detail' },
  zj = { command = 'common.next-section' },
  zk = { command = 'common.prev-section' },
}

local FOLD_ORDER = { 'za', 'zA', 'zo', 'zO', 'zc', 'zC', 'zR', 'zM', 'zr', 'zm', 'zj', 'zk' }

--- The fold target a user mapping's `rhs` mirrors, if any.
local function fold_target(rhs)
  if type(rhs) ~= 'string' or rhs == '' then
    return nil
  end
  local fold = rhs:match('^%s*:?%s*normal!?%s+([zZ][a-zA-Z])%s*$')
    or rhs:match('^%s*<Cmd>%s*normal!?%s+([zZ][a-zA-Z])%s*<CR>%s*$')
    or rhs:match('^([zZ][a-zA-Z])$')
  return fold and FOLDS[fold] or nil
end

--- User mappings that mirror a native fold command, keyed by their `lhs`.
---@param bufnr integer
---@return table<string, table>
local function user_folds(bufnr)
  local found = {}
  local maps = {}
  vim.list_extend(maps, vim.api.nvim_get_keymap('n'))
  vim.list_extend(maps, vim.api.nvim_buf_get_keymap(bufnr, 'n'))
  for _, map in ipairs(maps) do
    local target = fold_target(map.rhs)
    if target and not found[map.lhs] then
      found[map.lhs] = target
    end
  end
  return found
end

--- Remove every tracked map of `bufnr`.
---@param bufnr integer
function M.clear(bufnr)
  for _, entry in ipairs(M.applied[bufnr] or {}) do
    pcall(vim.keymap.del, entry.mode, entry.lhs, { buffer = bufnr })
  end
  M.applied[bufnr] = nil
end

--- Apply `notice` as the complete keymap of `bufnr`.
---
--- `handlers.press(steps)` forwards a chord's canonical steps; `handlers.command`
--- runs a server command (used by the fold mirrors). `opts.visit` overrides one
--- normal-mode `lhs` with a client-side function, and `opts.fold_keys` adds
--- explicit fold mirrors for Lua-function user mappings.
---
--- `opts.commands` maps a command id to a client-side function (an edit's
--- confirm/cancel), and `opts.only_commands` drops every binding without one
--- so a real file keeps its ordinary editor keys.
---@param bufnr integer
---@param notice table `view/keymap` params
---@param handlers table { press: fun(steps: string[]), command: fun(command: string, args: table|nil) }
---@param opts table|nil { visit?: table, fold_keys?: table<string, string>, folds?: boolean, commands?: table<string, function>, only_commands?: boolean }
---@return table[] applied entries
function M.apply(bufnr, notice, handlers, opts)
  opts = opts or {}
  local entries = {}
  local seen = {}

  local function add(mode, lhs, fn, desc)
    local key = mode .. '\0' .. lhs
    if seen[key] then
      return
    end
    seen[key] = true
    entries[#entries + 1] = { mode = mode, lhs = lhs, fn = fn, desc = desc }
  end

  local function drop_lhs(lhs)
    for index = #entries, 1, -1 do
      if entries[index].lhs == lhs then
        seen[entries[index].mode .. '\0' .. entries[index].lhs] = nil
        table.remove(entries, index)
      end
    end
  end

  local function command_fn(target)
    return function()
      handlers.command(target.command, target.args)
    end
  end

  -- Text mode is plain editing: only the server's toggle stays bound.
  local text_mode = notice.mode == 'text'

  -- Every binding the server sent, in each mode it applies to.
  for _, binding in ipairs(notice.bindings or {}) do
    local local_fn = opts.commands and opts.commands[binding.command]
    if local_fn or not opts.only_commands then
      for _, chord in ipairs(binding.keys or {}) do
        local lhs = keys.to_lhs(chord)
        local steps = keys.steps(chord)
        if lhs and steps then
          for _, mode in ipairs(binding.modes or { 'normal', 'visual' }) do
            local nvim_mode = mode == 'visual' and 'x' or 'n'
            local captured = steps
            add(nvim_mode, lhs, local_fn or function()
              handlers.press(captured)
            end, binding.label or binding.command)
          end
        end
      end
    end
  end

  -- Visit is a documented client-side exception until `common.visit` lands.
  if opts.visit and not text_mode then
    drop_lhs(opts.visit.lhs)
    add('n', opts.visit.lhs, opts.visit.fn, 'Visit file at line')
  end

  -- Mirror the native fold keys; a server binding for the same lhs wins.
  if not text_mode and opts.folds ~= false then
    for _, native in ipairs(FOLD_ORDER) do
      add('n', native, command_fn(FOLDS[native]), 'Fold')
    end
    for lhs, target in pairs(user_folds(bufnr)) do
      add('n', lhs, command_fn(target), 'Fold')
    end
    for lhs, command in pairs(opts.fold_keys or {}) do
      add('n', lhs, command_fn({ command = command }), 'Fold')
    end
  end

  -- Capture mode forwards every other printable ASCII key to the server.
  if notice.capture and not text_mode and not opts.only_commands then
    for code = 32, 126 do
      local char = string.char(code)
      local canonical = char
      if char == ' ' then
        canonical = 'Space'
      elseif char == '+' then
        canonical = 'Plus'
      end
      local lhs = keys.to_lhs(canonical)
      if lhs then
        local captured = { canonical }
        for _, nvim_mode in ipairs({ 'n', 'x' }) do
          add(nvim_mode, lhs, function()
            handlers.press(captured)
          end, 'Capture key')
        end
      end
    end
  end

  for _, entry in ipairs(M.applied[bufnr] or {}) do
    pcall(vim.keymap.del, entry.mode, entry.lhs, { buffer = bufnr })
  end
  for _, entry in ipairs(entries) do
    vim.keymap.set(entry.mode, entry.lhs, entry.fn, {
      buffer = bufnr,
      nowait = true,
      silent = true,
      desc = entry.desc,
    })
  end
  M.applied[bufnr] = entries
  return entries
end

return M
