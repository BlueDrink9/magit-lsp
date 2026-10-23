--- Prompt transport for the Neovim client (§3.6, R-3.1).
---
--- The server sends `view/prompt` as a notification and blocks until the
--- client answers with a `view/promptRespond` request. This module is the only
--- place the client interprets a prompt: it maps each kind onto Neovim's
--- `vim.ui`/`vim.fn` helpers and forwards the answer. It holds no Git or
--- layout logic.

local debug_api = require('magit-lsp.debug')

local M = {}

--- The LSP client id, set by `M.configure`.
M.client_id = nil

--- Called once the LSP client exists.
---@param opts table { client_id: integer }
function M.configure(opts)
  M.client_id = opts.client_id
end

local function client()
  if not M.client_id then
    return nil
  end
  return vim.lsp.get_client_by_id(M.client_id)
end

--- Answer a prompt; `cancelled` means the user dismissed it.
---@param prompt_id string
---@param cancelled boolean
---@param value any the kind-shaped answer, omitted when cancelled
function M.respond(prompt_id, cancelled, value)
  local c = client()
  if not c then
    debug_api.prompt_finished()
    return
  end
  c:request('view/promptRespond', {
    promptId = prompt_id,
    cancelled = cancelled,
    value = value,
  }, function() end)
  debug_api.prompt_finished()
end

local function finish(params, cancelled, value)
  M.respond(params.promptId, cancelled, value)
end

local function format_item(item)
  if item.description and item.description ~= '' then
    return string.format('%s — %s', item.label, item.description)
  end
  return item.label
end

--- Run a text prompt; `on_value` receives the typed string.
local function ask_text(params, on_value)
  if params.secret then
    -- `vim.ui.input` cannot mask, so secret answers use the built-in helper.
    local value = vim.fn.inputsecret(params.title)
    if value == nil or value == '' then
      finish(params, true)
    else
      on_value(value)
    end
    return
  end
  local default = type(params.default) == 'string' and params.default or nil
  vim.ui.input({ prompt = params.title, default = default }, function(input)
    if input == nil then
      finish(params, true)
    else
      on_value(input)
    end
  end)
end

local function ask_confirm(params)
  local yes = params.yesLabel or 'Yes'
  -- A destructive question focuses "No" first.
  local labels = params.destructive and { params.noLabel or 'No', yes }
    or { yes, params.noLabel or 'No' }
  vim.ui.select(labels, { prompt = params.title }, function(choice)
    if choice == nil then
      finish(params, true)
    else
      finish(params, false, choice == yes)
    end
  end)
end

local function ask_choice(params)
  local items = {}
  for _, item in ipairs(params.items or {}) do
    items[#items + 1] = item
  end
  if params.allowCustom then
    items[#items + 1] = { id = '__custom__', label = 'Enter custom value…' }
  end
  vim.ui.select(items, {
    prompt = params.title,
    format_item = format_item,
  }, function(item)
    if item == nil then
      finish(params, true)
    elseif item.id == '__custom__' then
      ask_text(params, function(text)
        finish(params, false, { custom = text })
      end)
    else
      finish(params, false, { id = item.id })
    end
  end)
end

local function ask_multi(params)
  local selected = {}
  local order = {}
  for _, id in ipairs(params.preselected or {}) do
    if not selected[id] then
      selected[id] = true
      order[#order + 1] = id
    end
  end

  local function step()
    local items = {}
    for _, item in ipairs(params.items or {}) do
      items[#items + 1] = {
        id = item.id,
        label = (selected[item.id] and '[x] ' or '[ ] ') .. item.label,
        description = item.description,
        detail = item.detail,
      }
    end
    items[#items + 1] = { id = '__done__', label = 'Done' }

    vim.ui.select(items, {
      prompt = params.title,
      format_item = format_item,
    }, function(item)
      if item == nil then
        finish(params, true)
      elseif item.id == '__done__' then
        finish(params, false, { ids = order })
      else
        if selected[item.id] then
          selected[item.id] = nil
          for index, id in ipairs(order) do
            if id == item.id then
              table.remove(order, index)
              break
            end
          end
        else
          selected[item.id] = true
          order[#order + 1] = item.id
        end
        step()
      end
    end)
  end

  step()
end

--- Show one `view/prompt` notification and answer it.
---@param params table `view/prompt` params
function M.handle(params)
  if type(params) ~= 'table' or not params.promptId then
    return
  end
  debug_api.prompt_started()
  local kind = params.kind
  if kind == 'confirm' then
    ask_confirm(params)
  elseif kind == 'text' then
    ask_text(params, function(text)
      finish(params, false, text)
    end)
  elseif kind == 'choice' then
    ask_choice(params)
  elseif kind == 'multiChoice' then
    ask_multi(params)
  else
    -- An unknown kind cannot be repaired client-side; cancel it.
    finish(params, true)
  end
end

return M
