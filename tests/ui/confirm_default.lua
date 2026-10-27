--- Exercise the real prompt adapter against a fake transport, not the server.
local fixture = vim.json.decode(table.concat(vim.fn.readfile(
  '../../../../../tests/data/client-contracts.json'), '\n'))
local contract
for _, prompt in ipairs(fixture.prompts) do
  if prompt.name == 'confirm-default-false' then contract = prompt end
end
assert(contract, 'missing confirm-default-false contract')

local clients = vim.lsp.get_clients()
assert(clients[1], 'missing UI client')
local client_id = clients[1].id
local lookup = vim.lsp.get_client_by_id
vim.lsp.get_client_by_id = function(id, ...)
  if id ~= client_id then return lookup(id, ...) end
  local original = lookup(id, ...)
  assert(original, 'missing originating UI client')
  return setmetatable({ request = function(_, method, params, callback)
  assert(method == 'view/promptRespond', 'unexpected fake-client request')
  vim.lsp.get_client_by_id = lookup
  assert(vim.deep_equal(params, {
    promptId = contract.params.promptId,
    cancelled = contract.answer.cancelled,
    value = contract.answer.value,
  }), 'prompt response differs from shared fixture')
  if callback then callback(nil, true) end
  vim.schedule(function()
    vim.cmd.redraw()
    vim.api.nvim_echo({ { 'False default declined' } }, false, {})
  end)
  return true
  end }, { __index = original })
end

require('magit-lsp.prompt').handle(contract.params, client_id)
