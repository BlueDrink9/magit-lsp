local MiniTest = require("mini.test")

--- Register menu, prompt and planned-action cases on the shared integration set.
return function(T, child, setup_child)
  --- Wait for the most recently opened menu and return its window state.
  local function wait_menu()
    local state = false
    local ok = vim.wait(5000, function()
      state = child.lua_get([==[(function()
      local v = require('magit-lsp.view')
      local id
      for index = #v.order, 1, -1 do
        local record = v.views[v.order[index]]
        if record and record.placement == 'bottom' then
          id = record.key
          break
        end
      end
      if not id then return false end
      local record = v.views[id]
      if not record or not record.win or not vim.api.nvim_win_is_valid(record.win)
        or record.version == nil or not record.keymap_ready then
        return false
      end
      local lines = vim.api.nvim_buf_get_lines(record.buf, 0, -1, false)
      return {
        id = id,
        win = record.win,
        buf = record.buf,
        config = vim.api.nvim_win_get_config(record.win),
        width = vim.api.nvim_win_get_width(record.win),
        height = vim.api.nvim_win_get_height(record.win),
        lines = lines,
        current = vim.api.nvim_get_current_win(),
        cap = math.max(1, vim.o.lines - vim.o.cmdheight - 1),
        split_cap = math.max(1, vim.o.lines - vim.o.cmdheight - 3),
        screen_lines = vim.o.lines,
        cmdheight = vim.o.cmdheight,
      }
    end)()]==])
      return type(state) == "table"
    end, 20)
    return ok and state or nil
  end

  --- The current lines of menu `id`, or an empty list when it closed.
  local function menu_lines(id)
    return child.lua_get(string.format(
      [[(function()
    local record = require('magit-lsp.view').views[%q]
    if not record then return {} end
    return vim.api.nvim_buf_get_lines(record.buf, 0, -1, false)
  end)()]],
      id
    ))
  end

  local function menu_closed(id)
    return child.lua_get(
      string.format("(function() return require('magit-lsp.view').views[%q] == nil end)()", id)
    )
  end

  local function has_prefix(lines, prefix)
    for _, line in ipairs(lines) do
      if vim.startswith(line, prefix) then
        return true
      end
    end
    return false
  end

  T["pressing c opens the commit menu in a bottom float"] = function()
    setup_child()
    child.type_keys(50, "c")
    local state = wait_menu()
    MiniTest.expect.equality(type(state), "table", "the commit menu opened")

    local height = math.min(#state.lines, state.cap)
    MiniTest.expect.equality(state.config.relative, "editor")
    MiniTest.expect.equality(state.config.row, state.screen_lines - height - state.cmdheight - 1)
    MiniTest.expect.equality(state.config.col, 0)
    MiniTest.expect.equality(state.config.width, child.lua_get("vim.o.columns"))
    MiniTest.expect.equality(state.config.height, height)
    MiniTest.expect.equality(state.current, state.win, "the menu is entered")

    MiniTest.expect.equality(state.lines[1], " Commit")
    MiniTest.expect.equality(
      vim.tbl_contains(state.lines, " Arguments"),
      true,
      "the arguments group"
    )
    MiniTest.expect.equality(vim.tbl_contains(state.lines, " git commit"), true, "the preview line")
    MiniTest.expect.equality(has_prefix(state.lines, "  c  Commit"), true, "the Commit action")

    child.type_keys(50, "-a")
    local toggled = vim.wait(5000, function()
      for _, line in ipairs(menu_lines(state.id)) do
        if line:find("%-%-all", 1) and line:find("%[x%]", 1) then
          return true
        end
      end
      return false
    end, 20)
    MiniTest.expect.equality(toggled, true, "the --all switch shows [x] on screen")

    child.type_keys(50, "<Esc>")
    MiniTest.expect.equality(
      vim.wait(5000, function()
        return menu_closed(state.id)
      end, 20),
      true,
      "Escape closed the menu"
    )
    MiniTest.expect.equality(
      child.lua_get("vim.api.nvim_get_current_win()"),
      child.lua_get("vim.fn.bufwinid(STATUS.buf)"),
      "focus returns to the status window"
    )
  end

  T["the longest menu fills the editor and scrolls when it is taller"] = function()
    setup_child()
    child.lua("vim.o.columns = 80; vim.o.lines = 24")
    -- The log menu is the longest at 80 columns; `l` opens it in the vim profile.
    child.type_keys(50, "l")
    local state = wait_menu()
    MiniTest.expect.equality(type(state), "table", "the log menu opened")
    MiniTest.expect.equality(state.width, 80)
    MiniTest.expect.equality(state.screen_lines, 24)

    local available = state.screen_lines - state.cmdheight - 1
    MiniTest.expect.equality(state.height, math.min(#state.lines, available))
    MiniTest.expect.equality(
      #state.lines > state.height,
      true,
      "the log menu is taller than the float"
    )

    -- C-e is not a captured menu key, so the float scrolls natively.
    child.type_keys(50, "<C-e>")
    local top = child.lua_get(
      string.format(
        '(function() return vim.api.nvim_win_call(%d, function() return vim.fn.line("w0") end) end)()',
        state.win
      )
    )
    MiniTest.expect.equality(top > 1, true, "C-e scrolls the menu float")

    child.api.nvim_win_set_cursor(state.win, { #state.lines, 0 })
    local reached = child.lua_get(
      string.format(
        '(function() return vim.api.nvim_win_call(%d, function() return vim.fn.line(".") == vim.fn.line("$") end) end)()',
        state.win
      )
    )
    MiniTest.expect.equality(reached, true, "the cursor reaches the last line")

    child.type_keys(50, "<Esc>")
    MiniTest.expect.equality(
      vim.wait(5000, function()
        return menu_closed(state.id)
      end, 20),
      true,
      "Escape closed the log menu"
    )
  end

  T["menu.style split opens a bottom split"] = function()
    setup_child({ menu = { style = "split" } })
    child.type_keys(50, "c")
    local state = wait_menu()
    MiniTest.expect.equality(type(state), "table", "the commit menu opened")

    local height = math.min(#state.lines, state.split_cap)
    MiniTest.expect.equality(state.config.relative, "")
    MiniTest.expect.equality(state.height, height)
    local bottom = child.lua_get(
      string.format("vim.fn.win_screenpos(%d)[1] + vim.fn.winheight(%d) - 1", state.win, state.win)
    )
    MiniTest.expect.equality(bottom, state.screen_lines - state.cmdheight - 1)

    child.type_keys(50, "<Esc>")
    MiniTest.expect.equality(
      vim.wait(5000, function()
        return menu_closed(state.id)
      end, 20),
      true,
      "Escape closed the split menu"
    )
  end

  --- Replace every string equal to `secret` with a marker, so a failed equality
  --- below never prints the synthetic secret into the failure diagnostics.
  local function redact(value, secret)
    if type(value) == "table" then
      local out = {}
      for key, item in pairs(value) do
        out[key] = redact(item, secret)
      end
      return out
    end
    if type(value) == "string" and value == secret then
      return "<redacted>"
    end
    return value
  end

  T["every prompt kind answers view/promptRespond with its §3.6 shape"] = function()
    setup_child()
    child.lua([[
    -- Drive one synthetic `view/prompt` through the real notification handler
    -- with a fake capturing client and per-row `vim.ui`/`vim.fn` stubs.
    RUN_PROMPT = function(scenario)
      local debug_api = require('magit-lsp.debug')
      local sent = {}
      local calls = { input = {}, select = {}, inputsecret = {} }
      local real_client = vim.lsp.get_client_by_id
      vim.lsp.get_client_by_id = function(id)
        if id == 777 then
          return {
            request = function(_, method, params, handler)
              sent[#sent + 1] = { method = method, params = params }
              if handler then handler(nil, {}) end
            end,
          }
        end
        return real_client(id)
      end
      local select_answers = {}
      for index, answer in ipairs(scenario.selectAnswers or {}) do
        select_answers[index] = answer
      end
      vim.ui.input = function(opts, on_confirm)
        calls.input[#calls.input + 1] = { prompt = opts.prompt, default = opts.default }
        on_confirm(scenario.inputAnswer)
      end
      vim.fn.inputsecret = function(title)
        calls.inputsecret[#calls.inputsecret + 1] = title
        return scenario.secretAnswer
      end
      vim.ui.select = function(items, opts, on_confirm)
        local entry = { prompt = opts.prompt, items = vim.deepcopy(items) }
        if opts.format_item then
          entry.formatted = vim.tbl_map(opts.format_item, items)
        end
        calls.select[#calls.select + 1] = entry
        local answer = table.remove(select_answers, 1)
        if answer == nil then
          on_confirm(nil)
          return
        end
        if type(answer) == 'number' then
          on_confirm(items[answer])
          return
        end
        for _, item in ipairs(items) do
          if (type(item) == 'table' and item.id or item) == answer then
            on_confirm(item)
            return
          end
        end
        on_confirm(nil)
      end
      vim.lsp.handlers['view/prompt'](nil, scenario.params, { client_id = 777 })
      vim.lsp.get_client_by_id = real_client
      return { sent = sent, calls = calls, prompts_pending = debug_api.state.prompts }
    end
  ]])

    local fixture =
      vim.json.decode(table.concat(vim.fn.readfile("tests/data/client-contracts.json"), "\n"))
    local scenarios = {}
    for _, vector in ipairs(fixture.prompts) do
      local scenario =
        { name = vector.name, params = vector.params, want = vim.deepcopy(vector.answer) }
      scenario.want.promptId = vector.params.promptId
      if vector.response == vim.NIL then
        scenario.selectAnswers = {}
      elseif vector.params.kind == "confirm" then
        scenario.selectAnswers = {
          (vector.focus and vector.focus[1] or "Yes") == "Yes"
              and (vector.response == "yes" and 1 or 2)
            or (vector.response == "yes" and 2 or 1),
        }
      elseif vector.params.kind == "choice" then
        scenario.selectAnswers = { vector.response }
        if vector.response == "__custom__" then
          scenario.inputAnswer = vector.customResponse ~= vim.NIL and vector.customResponse or nil
          scenario.want_input = { prompt = vector.params.title }
        end
      elseif vector.params.kind == "multiChoice" then
        scenario.selectAnswers = {}
        for _, id in ipairs(vector.response) do
          scenario.selectAnswers[#scenario.selectAnswers + 1] = id
        end
        scenario.selectAnswers[#scenario.selectAnswers + 1] = "__done__"
      else
        scenario.inputAnswer = vector.response ~= vim.NIL and vector.response or nil
        if vector.params.secret then
          scenario.secretAnswer = vector.response
          scenario.want_secret_titles = { vector.params.title }
        else
          scenario.want_input = { prompt = vector.params.title, default = vector.params.default }
          if vector.params.default == nil then
            scenario.want_input.default = nil
          end
        end
      end
      if vector.focus then
        scenario.want_focus = vector.focus
      end
      scenarios[#scenarios + 1] = scenario
    end
    local multi_items = {
      { id = "a", label = "a" },
      { id = "b", label = "b", description = "desc b" },
      { id = "c", label = "c" },
    }
    scenarios[#scenarios + 1] = {
      name = "multiChoice renders and toggles checklist",
      params = {
        promptId = "local-multi",
        kind = "multiChoice",
        title = "Pick branches",
        items = multi_items,
        preselected = { "a" },
      },
      selectAnswers = { "b", "a", "__done__" },
      want = { promptId = "local-multi", cancelled = false, value = { ids = { "b" } } },
      want_select_steps = {
        { "[x] a", "[ ] b — desc b", "[ ] c", "Done" },
        { "[x] a", "[x] b — desc b", "[ ] c", "Done" },
        { "[ ] a", "[x] b — desc b", "[ ] c", "Done" },
      },
    }
    scenarios[#scenarios + 1] = {
      name = "secret success",
      params = { promptId = "p-secret-ok", kind = "text", title = "SSH passphrase", secret = true },
      secretAnswer = "correct horse battery",
      secret = "correct horse battery",
      want = { promptId = "p-secret-ok", cancelled = false, value = "<redacted>" },
      want_secret_titles = { "SSH passphrase" },
      want_no_input = true,
    }
    scenarios[#scenarios + 1] = {
      name = "secret empty answer cancels",
      params = {
        promptId = "p-secret-empty",
        kind = "text",
        title = "SSH passphrase",
        secret = true,
      },
      secretAnswer = "",
      want = { promptId = "p-secret-empty", cancelled = true, value = nil },
      want_secret_titles = { "SSH passphrase" },
      want_no_input = true,
    }
    scenarios[#scenarios + 1] = {
      name = "secret cancel",
      params = {
        promptId = "p-secret-cancel",
        kind = "text",
        title = "SSH passphrase",
        secret = true,
      },
      want = { promptId = "p-secret-cancel", cancelled = true, value = nil },
      want_secret_titles = { "SSH passphrase" },
      want_no_input = true,
    }

    for _, scenario in ipairs(scenarios) do
      local label = scenario.name
      child.lua(string.format("RESULT = RUN_PROMPT(%s)", vim.inspect(scenario)))
      local result = child.lua_get("RESULT")
      MiniTest.expect.equality(type(result), "table", label .. ": the prompt was handled")
      if type(result) == "table" then
        MiniTest.expect.equality(#result.sent, 1, label .. ": exactly one respond request")
        if result.sent[1] then
          MiniTest.expect.equality(
            result.sent[1].method,
            "view/promptRespond",
            label .. ": the answer method"
          )
          local got, want = result.sent[1].params, scenario.want
          if scenario.secret then
            got, want = redact(got, scenario.secret), redact(want, scenario.secret)
          end
          MiniTest.expect.equality(got, want, label .. ": the respond payload")
        end
        MiniTest.expect.equality(
          result.prompts_pending,
          0,
          label .. ": the prompt counter is back to zero"
        )

        if scenario.want_input then
          MiniTest.expect.equality(
            result.calls.input[1],
            scenario.want_input,
            label .. ": the vim.ui.input options"
          )
          MiniTest.expect.equality(#result.calls.input, 1, label .. ": one vim.ui.input call")
        end
        if scenario.want_no_input then
          MiniTest.expect.equality(#result.calls.input, 0, label .. ": vim.ui.input was never used")
        end
        if scenario.want_secret_titles then
          MiniTest.expect.equality(
            result.calls.inputsecret,
            scenario.want_secret_titles,
            label .. ": inputsecret got the title"
          )
        else
          MiniTest.expect.equality(
            #result.calls.inputsecret,
            0,
            label .. ": inputsecret was never used"
          )
        end

        if scenario.want_select_items then
          MiniTest.expect.equality(
            result.calls.select[1].items,
            scenario.want_select_items,
            label .. ": the select items"
          )
          MiniTest.expect.equality(
            result.calls.select[1].prompt,
            scenario.params.title,
            label .. ": the select prompt"
          )
        end
        if scenario.want_focus then
          local labels = {}
          for _, item in ipairs(result.calls.select[1].items) do
            labels[#labels + 1] = type(item) == "table" and item.label or item
          end
          MiniTest.expect.equality(labels, scenario.want_focus, label .. ": focus label order")
        end
        if scenario.want_select_formatted then
          MiniTest.expect.equality(
            result.calls.select[1].formatted,
            scenario.want_select_formatted,
            label .. ": the rendered item labels"
          )
        end
        if scenario.want_select_steps then
          MiniTest.expect.equality(
            #result.calls.select,
            #scenario.want_select_steps,
            label .. ": one select call per toggle step"
          )
          for step, want in ipairs(scenario.want_select_steps) do
            MiniTest.expect.equality(
              result.calls.select[step].formatted,
              want,
              label .. ": the step " .. step .. " labels"
            )
            MiniTest.expect.equality(
              result.calls.select[step].prompt,
              scenario.params.title,
              label .. ": the step " .. step .. " prompt"
            )
          end
        end
      end
    end

    MiniTest.expect.equality(
      child.lua(
        'return vim.wait(2000, function() return require("magit-lsp.debug").idle() end, 10)'
      ),
      true,
      "the client is back to idle"
    )
  end

  T["overlapping and nested prompts retain their originating client"] = function()
    setup_child()
    child.lua([[
    local sent = { [11] = {}, [22] = {} }
    local connected = { [11] = true, [22] = true }
    vim.lsp.get_client_by_id = function(id)
      if connected[id] then
        return { request = function(_, method, params)
          table.insert(sent[id], { method = method, params = params })
        end }
      end
    end
    local selects, inputs = {}, {}
    vim.ui.select = function(items, _, callback)
      table.insert(selects, { items = items, callback = callback })
    end
    vim.ui.input = function(_, callback) table.insert(inputs, callback) end
    local handler = vim.lsp.handlers['view/prompt']
    handler(nil, { promptId = 'first', kind = 'confirm' }, { client_id = 11 })
    handler(nil, { promptId = 'second', kind = 'confirm' }, { client_id = 22 })
    selects[2].callback(selects[2].items[1])
    selects[1].callback(selects[1].items[1])

    handler(nil, { promptId = 'nested', kind = 'choice', allowCustom = true,
      items = { { id = 'custom', label = 'Custom' } } }, { client_id = 11 })
    selects[3].callback(selects[3].items[2])
    handler(nil, { promptId = 'other', kind = 'confirm' }, { client_id = 22 })
    selects[4].callback(selects[4].items[1])
    inputs[1]('nested answer')

    handler(nil, { promptId = 'missing', kind = 'confirm' }, nil)
    selects[5].callback(selects[5].items[1])
    connected[11] = false
    handler(nil, { promptId = 'disconnected', kind = 'confirm' }, { client_id = 11 })
    selects[6].callback(selects[6].items[1])
    _G.PROMPT_OWNERSHIP = sent
  ]])
    local sent = child.lua_get("PROMPT_OWNERSHIP")
    MiniTest.expect.equality(sent[11][1].params.promptId, "first")
    MiniTest.expect.equality(sent[22][1].params.promptId, "second")
    MiniTest.expect.equality(sent[11][2].params.promptId, "nested")
    MiniTest.expect.equality(sent[11][2].params.value.custom, "nested answer")
    MiniTest.expect.equality(sent[22][2].params.promptId, "other")
    MiniTest.expect.equality(#sent[11], 2, "missing and disconnected clients do not redirect")
    MiniTest.expect.equality(#sent[22], 2, "no response is sent through another client")
  end

  T["a menu option prompt asks through vim.ui.input and lands the value"] = function()
    setup_child()
    child.lua([[
    PROMPTS = {}
    vim.ui.input = function(opts, on_confirm)
      table.insert(PROMPTS, opts.prompt)
      on_confirm('Alice')
    end
  ]])

    child.type_keys(50, "c")
    local state = wait_menu()
    MiniTest.expect.equality(type(state), "table", "the commit menu opened")

    child.type_keys(50, "=A")
    local applied = vim.wait(5000, function()
      for _, line in ipairs(menu_lines(state.id)) do
        if line:find("--author=Alice", 1, true) then
          return true
        end
      end
      return false
    end, 20)
    MiniTest.expect.equality(applied, true, "the option value and preview land")
    MiniTest.expect.equality(child.lua_get("#PROMPTS"), 1, "one prompt was shown")
    MiniTest.expect.equality(child.lua_get("PROMPTS[1]"), "Override the author (--author=)")
  end

  --- Compatibility alias for `unpack` across Lua versions.
  local unpack = unpack

  --- Fetch the structured menu model of the view `state` from the server via
  --- `view/resync`. The wire request uses the actual `record.id`; the composite
  --- `record.key` is only used to look the view record up on the client.
  ---@param state table the `wait_menu()` state of an open menu
  ---@return table|nil menu the server's menu model, or nil
  local function menu_model(state)
    if not state then
      return nil
    end
    return child.lua_get(string.format(
      [[
    (function()
      local record = require('magit-lsp.view').views[%q]
      if not record then return nil end
      local client = vim.lsp.get_client_by_id(record.client_id)
      local done, result
      client:request('view/resync', { viewId = record.id }, function(err, value)
        result = value
        done = true
      end, record.buf)
      vim.wait(5000, function() return done end, 10)
      if not result then return nil end
      return result.menu
    end)()
  ]],
      state.id
    ))
  end

  --- The first planned action of the menu model, with its key, label and roadmap
  --- id derived from the server metadata.
  ---@param state table the `wait_menu()` state of an open menu
  ---@return table|nil planned { key, label, roadmap }
  local function planned_in(state)
    local menu = menu_model(state)
    if not menu then
      return nil
    end
    for _, group in ipairs(menu.groups or {}) do
      for _, item in ipairs(group.items or {}) do
        if item.status and item.status.status == "planned" then
          return { key = item.key, label = item.label, roadmap = item.status.roadmapId }
        end
      end
    end
    return nil
  end

  --- Find a real planned menu action without pinning any menu, key or label.
  ---
  --- Tries the commit menu first; when it has no planned action, opens the
  --- dispatch menu and tries every opener it lists, so the cases keep working
  --- once every commit action is implemented.
  ---@return table|nil planned { key, label, roadmap }
  ---@return table|nil state the menu state the planned action was found in
  local function find_planned_action()
    child.type_keys(50, "c")
    local state = wait_menu()
    local planned = planned_in(state)
    if planned then
      return planned, state
    end

    child.type_keys(50, "q")
    child.type_keys(50, "g?")
    local dispatch = wait_menu()
    planned = planned_in(dispatch)
    if planned then
      return planned, dispatch
    end
    if not dispatch then
      return nil
    end

    local dispatch_model = menu_model(dispatch)
    if not dispatch_model then
      return nil
    end
    for _, group in ipairs(dispatch_model.groups or {}) do
      for _, item in ipairs(group.items or {}) do
        if item.kind == "action" and item.key then
          local keys = {}
          for key in item.key:gmatch("%S+") do
            keys[#keys + 1] = key
          end
          child.type_keys(50, unpack(keys))
          local opened = wait_menu()
          local found = planned_in(opened)
          if found then
            return found, opened
          end
          if opened then
            child.type_keys(50, "q")
            child.type_keys(50, "g?")
            wait_menu()
          end
        end
      end
    end
    child.type_keys(50, "q")
    return nil
  end

  --- Locate the `magit.menu.planned` token for `label` through the server's
  --- semantic-token ranges, never by searching the rendered labels.
  ---@param view_id string the composite key of the open menu view
  ---@param label string the label of the planned action to locate
  ---@return table|nil position { row, col } of the planned token
  local function planned_token_position(view_id, label)
    return child.lua_get(string.format(
      [[
    (function()
      local view = require('magit-lsp.view').views[%q]
      if not view then return nil end
      local get_clients = vim.lsp.get_clients or vim.lsp.get_active_clients
      local client = get_clients({ bufnr = view.buf })[1]
      if not client then return nil end
      local capability = client.server_capabilities.semanticTokensProvider
      if not capability then return nil end
      local responses = vim.lsp.buf_request_sync(view.buf,
        'textDocument/semanticTokens/full',
        { textDocument = { uri = view.uri } }, 5000)
      local response = responses and responses[client.id]
      if not response or not response.result then return nil end
      local data = response.result.data
      local line, char = 0, 0
      for i = 1, #data, 5 do
        local delta_line, delta_start, length, token_type =
          data[i], data[i + 1], data[i + 2], data[i + 3]
        line = line + delta_line
        char = delta_line > 0 and delta_start or char + delta_start
        if capability.legend.tokenTypes[token_type + 1] == 'magit.menu.planned'
          and vim.api.nvim_buf_get_text(
            view.buf, line, char, line, char + length, {})[1] == %q then
          return { row = line, col = char }
        end
      end
      return nil
    end)()
  ]],
      view_id,
      label
    ))
  end

  T["planned menu actions are greyed with the menu.planned token"] = function()
    setup_child()
    local planned, state = find_planned_action()
    MiniTest.expect.equality(type(state), "table", "a menu with a planned action opened")

    local position = planned_token_position(state.id, planned.label)
    MiniTest.expect.equality(type(position), "table", "the planned action token position")

    local token = child.lua_get(string.format(
      [[
    (function()
      local view = require('magit-lsp.view').views[%q]
      local ok = vim.wait(5000, function()
        return vim.lsp.semantic_tokens.get_at_pos(view.buf, %d, %d) ~= nil
      end, 20)
      if not ok then return nil end
      local tokens = vim.lsp.semantic_tokens.get_at_pos(view.buf, %d, %d)
      return tokens and tokens[1] and tokens[1].type or nil
    end)()
  ]],
      state.id,
      position.row,
      position.col,
      position.row,
      position.col
    ))
    MiniTest.expect.equality(token, "magit.menu.planned")
  end

  T["a planned menu action notifies once and keeps the menu open"] = function()
    setup_child()
    local planned, state = find_planned_action()
    MiniTest.expect.equality(type(state), "table", "a menu with a planned action opened")

    child.lua([[
    NOTIFIES = {}
    vim.notify = function(msg, level)
      table.insert(NOTIFIES, { msg = msg, level = level })
    end
  ]])
    local keys = {}
    for key in planned.key:gmatch("%S+") do
      keys[#keys + 1] = key
    end
    child.type_keys(50, unpack(keys))
    vim.wait(5000, function()
      return child.lua_get("#NOTIFIES") > 0
    end, 20)
    MiniTest.expect.equality(child.lua_get("#NOTIFIES"), 1)
    MiniTest.expect.equality(
      child.lua_get("NOTIFIES[1].msg"),
      "Not implemented yet: " .. planned.label .. " (roadmap " .. planned.roadmap .. ")"
    )
    MiniTest.expect.equality(child.lua_get("NOTIFIES[1].level"), vim.log.levels.INFO)
    MiniTest.expect.equality(menu_closed(state.id), false, "the menu stays open")
  end

  T["the dispatch menu lists the active profile keys"] = function()
    setup_child()
    child.type_keys(50, "g?")
    local state = wait_menu()
    MiniTest.expect.equality(type(state), "table", "the dispatch menu opened")
    local lines = menu_lines(state.id)
    local function has(text)
      for _, line in ipairs(lines) do
        if line:find(text, 1, true) then
          return true
        end
      end
      return false
    end
    MiniTest.expect.equality(has("  Z  Stash"), true, "vim binds stash to Z")
    MiniTest.expect.equality(has("  l  Log"), true, "vim binds log to l")
    MiniTest.expect.equality(has("  c  Commit"), true, "vim binds commit to c")
    MiniTest.expect.equality(has("  g ?  Dispatch"), false, "dispatch does not list itself")
  end

  T["? from a menu replaces it and leaves one float"] = function()
    setup_child()
    child.type_keys(50, "c")
    local commit = wait_menu()
    MiniTest.expect.equality(type(commit), "table", "the commit menu opened")

    child.type_keys(50, "?")
    local state
    local replaced = vim.wait(5000, function()
      state = child.lua_get(string.format(
        [==[(function()
      local view = require('magit-lsp.view')
      local debug_api = require('magit-lsp.debug')
      local snapshot = debug_api.snapshot()
      local floats = 0
      for _, window in ipairs(snapshot.windows) do
        if window.float then floats = floats + 1 end
      end
      local last
      for index = #view.order, 1, -1 do
        local record = view.views[view.order[index]]
        if record and record.placement == 'bottom' then
          last = record.key
          break
        end
      end
      return {
        floats = floats,
        last = last,
        old_closed = view.views[%q] == nil,
        current = vim.api.nvim_get_current_win(),
      }
    end)()]==],
        commit.id
      ))
      return type(state) == "table"
        and state.floats == 1
        and state.old_closed
        and state.last ~= commit.id
    end, 20)
    MiniTest.expect.equality(replaced, true, "the commit menu was replaced by one menu")
    MiniTest.expect.equality(
      state.last:find("menu.dispatch", 1, true) ~= nil,
      true,
      "the dispatch menu took its place"
    )

    child.type_keys(50, "q")
    local closed = vim.wait(5000, function()
      local snapshot = child.lua_get([[(function()
      local view = require('magit-lsp.view')
      local debug_api = require('magit-lsp.debug')
      local state = debug_api.snapshot()
      local floats = 0
      for _, window in ipairs(state.windows) do
        if window.float then floats = floats + 1 end
      end
      local open = 0
      for _, id in ipairs(view.order) do
        local record = view.views[id]
        if record and record.placement == 'bottom' then
          open = open + 1
        end
      end
      return { floats = floats, open = open }
    end)()]])
      return type(snapshot) == "table" and snapshot.floats == 0 and snapshot.open == 0
    end, 20)
    MiniTest.expect.equality(closed, true, "q closes the only menu")
    MiniTest.expect.equality(
      child.lua_get("vim.api.nvim_get_current_win()"),
      child.lua_get("vim.fn.bufwinid(STATUS.buf)"),
      "focus returns to the status window"
    )
  end

  return wait_menu
end
