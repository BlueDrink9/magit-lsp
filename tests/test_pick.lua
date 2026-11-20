local MiniTest = require("mini.test")
local T = MiniTest.new_set()

local pick = require("magit-lsp.pick")

--- Load the shared client-contract pick vectors.
---@return table[]
local function load_vectors()
  local text = table.concat(vim.fn.readfile("tests/data/client-contracts.json"), "\n")
  return vim.json.decode(text).picks or {}
end

--- Run one pick with capturing stubs, restoring them afterwards.
---@param params table `view/pick` params
---@param response integer|nil 1-based item index; nil cancels
---@return table result { sent, select, notified }
local function run_pick(params, response)
  local sent, select_calls, notified = {}, {}, {}
  local real_select = vim.ui.select
  local real_notify = vim.notify
  local real_client = vim.lsp.get_client_by_id
  vim.lsp.get_client_by_id = function(id)
    if id == 777 then
      return {
        request = function(_, method, request_params, handler)
          sent[#sent + 1] = { method = method, params = request_params }
          if handler then
            handler(nil, {})
          end
        end,
      }
    end
    return real_client(id)
  end
  vim.ui.select = function(items, select_opts, on_choice)
    select_calls[#select_calls + 1] = { items = vim.deepcopy(items), opts = select_opts }
    on_choice(response and items[response] or nil)
  end
  vim.notify = function(message, level)
    notified[#notified + 1] = { message = message, level = level }
  end
  local ok, err = pcall(pick.handle, params, 777)
  vim.ui.select = real_select
  vim.notify = real_notify
  vim.lsp.get_client_by_id = real_client
  if not ok then
    error(err, 0)
  end
  return { sent = sent, select = select_calls, notified = notified }
end

T["shared pick vectors answer view/pickRespond exactly"] = function()
  local vectors = load_vectors()
  MiniTest.expect.equality(#vectors >= 3, true, "shared pick vectors loaded")
  for _, vector in ipairs(vectors) do
    local label = vector.name
    local response = vector.response == vim.NIL and nil or vector.response
    local result = run_pick(vector.params, response)
    MiniTest.expect.equality(#result.sent, 1, label .. ": one response request")
    MiniTest.expect.equality(result.sent[1].method, "view/pickRespond", label .. ": method")
    MiniTest.expect.equality(result.sent[1].params, {
      pickId = vector.params.pickId,
      cancelled = vector.expected.cancelled,
      selected = vector.expected.selected,
    }, label .. ": respond params")
    MiniTest.expect.equality(#result.select, 1, label .. ": one picker")
    MiniTest.expect.equality(
      result.select[1].opts.prompt,
      vector.params.prompt,
      label .. ": picker prompt"
    )
    MiniTest.expect.equality(
      vim.tbl_map(result.select[1].opts.format_item, result.select[1].items),
      vim.tbl_map(function(item)
        return item.label
      end, vector.params.items),
      label .. ": picker labels"
    )
  end
end

T["cancelling a pick sends an empty selection"] = function()
  local result = run_pick({
    pickId = "pick-cancel-explicit",
    prompt = "Pick",
    items = { { id = "a", label = "A" } },
  }, nil)
  MiniTest.expect.equality(result.sent[1].params, {
    pickId = "pick-cancel-explicit",
    cancelled = true,
    selected = {},
  })
end

T["undeclared pick shapes warn once and cancel without a picker"] = function()
  local long_items = {}
  for index = 1, 101 do
    long_items[index] = { id = "i" .. index, label = "item " .. index }
  end
  local cases = {
    {
      name = "multi",
      params = {
        pickId = "pick-multi",
        prompt = "Pick",
        multi = true,
        items = { { id = "a", label = "A" } },
      },
    },
    {
      name = "free text",
      params = {
        pickId = "pick-text",
        prompt = "Pick",
        allowFreeText = true,
        items = { { id = "a", label = "A" } },
      },
    },
    {
      name = "too many items",
      params = { pickId = "pick-long", prompt = "Pick", items = long_items },
    },
  }
  for _, case in ipairs(cases) do
    local result = run_pick(case.params, 1)
    MiniTest.expect.equality(#result.select, 0, case.name .. ": no picker")
    MiniTest.expect.equality(#result.notified, 1, case.name .. ": one warning")
    MiniTest.expect.equality(
      result.notified[1].level,
      vim.log.levels.WARN,
      case.name .. ": warning level"
    )
    MiniTest.expect.equality(#result.sent, 1, case.name .. ": one response request")
    MiniTest.expect.equality(result.sent[1].params, {
      pickId = case.params.pickId,
      cancelled = true,
      selected = {},
    }, case.name .. ": cancelled response")
  end
end

return T
