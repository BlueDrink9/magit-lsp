--- Pick transport for the Neovim client (§3.6, T008.8).
---
--- The server sends `view/pick` as a notification and waits for the client's
--- `view/pickRespond` request. This module maps a single-select pick onto
--- `vim.ui.select` and forwards the answer. It holds no Git or layout logic.

local M = {}

--- The largest single-select list this client presents.
local MAX_ITEMS = 100

--- Answer a pick through the client that originated it.
---
--- Cancelling always sends an empty selection, so the server never matches a
--- stale item id against a cancelled pick.
---@param client_id integer|nil originating LSP client
---@param pick_id string
---@param cancelled boolean
---@param selected string[]|nil item ids, omitted when cancelled
function M.respond(client_id, pick_id, cancelled, selected)
  local c = client_id and vim.lsp.get_client_by_id(client_id)
  if not c then
    return
  end
  c:request("view/pickRespond", {
    pickId = pick_id,
    cancelled = cancelled,
    selected = cancelled and {} or (selected or {}),
  }, function() end)
end

local function format_item(item)
  return item.label
end

--- Show one `view/pick` notification and answer it.
---
--- Declared shapes cover single-select only: a pick asking for multi-select or
--- free text, or one too long to present, warns once and cancels instead of
--- opening the picker.
---@param params table `view/pick` params
---@param client_id integer|nil originating LSP client
function M.handle(params, client_id)
  if type(params) ~= "table" or type(params.pickId) ~= "string" or params.pickId == "" then
    return
  end
  local items = type(params.items) == "table" and params.items or {}
  local reason
  if params.multi == true then
    reason = "multi-select picks are not supported"
  elseif params.allowFreeText == true then
    reason = "free-text picks are not supported"
  elseif #items > MAX_ITEMS then
    reason = string.format("picks are limited to %d items", MAX_ITEMS)
  end
  if reason then
    vim.notify("magit-lsp: " .. reason .. "; the pick was cancelled.", vim.log.levels.WARN)
    M.respond(client_id, params.pickId, true)
    return
  end
  vim.ui.select(items, {
    prompt = params.prompt,
    format_item = format_item,
  }, function(item)
    if item == nil then
      M.respond(client_id, params.pickId, true)
    else
      M.respond(client_id, params.pickId, false, { item.id })
    end
  end)
end

return M
