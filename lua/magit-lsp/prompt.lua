--- Prompt transport for the Neovim client (§3.6, R-3.1).
---
--- The server sends `view/prompt` as a notification and blocks until the
--- client answers with a `view/promptRespond` request. This module is the only
--- place the client interprets a prompt: it maps each kind onto Neovim's
--- `vim.ui`/`vim.fn` helpers and forwards the answer. It holds no Git or
--- layout logic.

local hooks = require('magit-lsp.hooks')

local M = {}

local function client(client_id)
  if not client_id then
    return nil
  end
  return vim.lsp.get_client_by_id(client_id)
end

--- Answer a prompt through the client that originated it.
---@param client_id integer|nil originating LSP client
---@param prompt_id string
---@param cancelled boolean
---@param value any the kind-shaped answer, omitted when cancelled
function M.respond(client_id, prompt_id, cancelled, value)
  local c = client(client_id)
  if not c then
    hooks.prompt_finished()
    return
  end
  c:request('view/promptRespond', {
    promptId = prompt_id,
    cancelled = cancelled,
    value = value,
  }, function() end)
  hooks.prompt_finished()
end

local function finish(client_id, params, cancelled, value)
  M.respond(client_id, params.promptId, cancelled, value)
end

local function format_item(item)
  if item.description and item.description ~= '' then
    return string.format('%s — %s', item.label, item.description)
  end
  return item.label
end

--- Run a text prompt; `on_value` receives the typed string.
local function ask_text(client_id, params, on_value)
  if params.secret then
    -- `vim.ui.input` cannot mask, so secret answers use the built-in helper.
    local value = vim.fn.inputsecret(params.title)
    if value == nil or value == '' then
      finish(client_id, params, true)
    else
      on_value(value)
    end
    return
  end
  local default = type(params.default) == 'string' and params.default or nil
  vim.ui.input({ prompt = params.title, default = default }, function(input)
    if input == nil then
      finish(client_id, params, true)
    else
      on_value(input)
    end
  end)
end

local function ask_confirm(client_id, params)
  local yes = params.yesLabel or 'Yes'
  -- A destructive question focuses "No" first.
  local no = params.noLabel or 'No'
  local labels = (params.destructive or params.default == false) and { no, yes }
    or { yes, no }
  vim.ui.select(labels, { prompt = params.title }, function(choice)
    if choice == nil then
      finish(client_id, params, true)
    else
      finish(client_id, params, false, choice == yes)
    end
  end)
end

local function ask_choice(client_id, params)
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
      finish(client_id, params, true)
    elseif item.id == '__custom__' then
      ask_text(client_id, params, function(text)
        finish(client_id, params, false, { custom = text })
      end)
    else
      finish(client_id, params, false, { id = item.id })
    end
  end)
end

local function ask_multi(client_id, params)
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
        finish(client_id, params, true)
      elseif item.id == '__done__' then
        finish(client_id, params, false, { ids = order })
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
---@param client_id integer|nil originating LSP client
function M.handle(params, client_id)
  if type(params) ~= 'table' or not params.promptId then
    return
  end
  hooks.prompt_started()
  local kind = params.kind
  if kind == 'confirm' then
    ask_confirm(client_id, params)
  elseif kind == 'text' then
    ask_text(client_id, params, function(text)
      finish(client_id, params, false, text)
    end)
  elseif kind == 'choice' then
    ask_choice(client_id, params)
  elseif kind == 'multiChoice' then
    ask_multi(client_id, params)
  else
    -- An unknown kind cannot be repaired client-side; cancel it.
    finish(client_id, params, true)
  end
end

return M
