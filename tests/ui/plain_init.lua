-- Standalone standard-LSP init for the Plain native smoke. No project Lua modules.
-- Test-only editor options and process plumbing; user config is in the guide.
vim.opt.swapfile = false
vim.opt.undofile = false
vim.opt.hidden = true
vim.opt.shortmess:append("I")
-- A stock `window/showMessage` can wrap past 'cmdheight' and raise a hit-enter
-- prompt, which blocks the driver's remote state requests. Keep messages in
-- history without pausing the editor; scenarios still observe them.
vim.o.messagesopt = "history:500,progress:c,wait:0"
vim.g.plain_smoke_rich_notifications = 0
vim.lsp.log.set_level("trace")
if vim.env.MAGIT_LSP_SERVER_WRAPPER then
  vim.env.MAGIT_LSP_BIN = vim.env.MAGIT_LSP_SERVER_WRAPPER
end

-- Load the exact plugin-free configuration published in the install guide, so
-- this harness never maintains a second copy of the user-facing setup.
local function documented_plain_config()
  local source = debug.getinfo(1, "S").source:sub(2)
  local doc = vim.fn.fnamemodify(source, ":h:h:h") .. "/docs/CLIENT_INSTALL.md"
  local marker = "<!-- plugin-free-plain-config -->"
  local block, in_fence, seen_marker = {}, false, false
  for line in io.lines(doc) do
    if line:find(marker, 1, true) then
      seen_marker = true
    elseif seen_marker and not in_fence and line == "```lua" then
      in_fence = true
    elseif in_fence and line == "```" then
      break
    elseif in_fence then
      block[#block + 1] = line
    end
  end
  assert(seen_marker, "documented Plain config marker missing from " .. doc)
  assert(#block > 0, "documented Plain config block missing from " .. doc)
  return table.concat(block, "\n")
end
local documented_config, documented_error = load(documented_plain_config(), "@CLIENT_INSTALL.md")
assert(documented_config, documented_error)
documented_config()
if vim.env.MAGIT_LSP_UI_PROBE_FILE then
  local probe_path = vim.env.MAGIT_LSP_UI_PROBE_FILE
  local function write_probe(record)
    _G.plain_probe_sequence = (_G.plain_probe_sequence or 0) + 1
    record.sequence = _G.plain_probe_sequence
    record.time_ns = vim.uv.hrtime()
    local file = assert(io.open(probe_path, "a"))
    file:write(vim.json.encode(record), "\n")
    file:close()
  end
  local function diagnostic_buffer(buf)
    local name = vim.api.nvim_buf_get_name(buf or vim.api.nvim_get_current_buf())
    return name:match("/%.magit$") or name:match("/COMMIT_EDITMSG$")
  end
  local original_print = print
  _G.print = function(...)
    local message = table.concat(vim.tbl_map(tostring, { ... }), "\t")
    local buf = vim.api.nvim_get_current_buf()
    if diagnostic_buffer(buf) and message:find("Buffer .* newer than edits%.") then
      write_probe({
        event = "warning-print-before",
        message = message,
        traceback = debug.traceback(),
        mode = vim.api.nvim_get_mode(),
        cmdtype = vim.fn.getcmdtype(),
      })
    end
    local result = original_print(...)
    if diagnostic_buffer(buf) and message:find("Buffer .* newer than edits%.") then
      write_probe({
        event = "warning-print-after",
        message = message,
        mode = vim.api.nvim_get_mode(),
        cmdtype = vim.fn.getcmdtype(),
      })
    end
    return result
  end
  local original_notify = vim.notify
  vim.notify = function(message, ...)
    local buf = vim.api.nvim_get_current_buf()
    local scoped = diagnostic_buffer(buf) and tostring(message):find("Buffer .* newer than edits%.")
    if scoped then
      write_probe({
        event = "warning-notify-before",
        message = tostring(message),
        traceback = debug.traceback(),
        mode = vim.api.nvim_get_mode(),
        cmdtype = vim.fn.getcmdtype(),
      })
    end
    local result = original_notify(message, ...)
    if scoped then
      write_probe({
        event = "warning-notify-after",
        message = tostring(message),
        mode = vim.api.nvim_get_mode(),
        cmdtype = vim.fn.getcmdtype(),
      })
    end
    return result
  end
  local function probe(event, key)
    local buf = vim.api.nvim_get_current_buf()
    local name = vim.api.nvim_buf_get_name(buf)
    if not name:match("/COMMIT_EDITMSG$") then
      return
    end
    local client = vim.lsp.get_client_by_id(vim.g.plain_smoke_client_id or -1)
    local autocmds = {}
    for _, autocmd_event in ipairs({
      "InsertEnter",
      "InsertLeave",
      "TextChanged",
      "TextChangedI",
      "BufWritePre",
      "BufWritePost",
    }) do
      for _, autocmd in ipairs(vim.api.nvim_get_autocmds({ event = autocmd_event })) do
        autocmds[#autocmds + 1] = {
          event = autocmd_event,
          desc = autocmd.desc,
          command = autocmd.command,
        }
      end
    end
    local cursor = vim.api.nvim_win_get_cursor(0)
    local messages = vim.fn.execute("messages")
    local mode_state = vim.api.nvim_get_mode()
    local record = {
      time_ns = vim.uv.hrtime(),
      event = event,
      key = key,
      keytrans = key and vim.fn.keytrans(key) or nil,
      text = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), "\n"),
      mode = mode_state.mode,
      blocking = mode_state.blocking,
      cursor = { row = cursor[1] - 1, col = cursor[2] },
      cmdtype = vim.fn.getcmdtype(),
      messages = messages,
      changedtick = vim.api.nvim_buf_get_changedtick(buf),
      uri = vim.uri_from_bufnr(buf),
      version = client and vim.lsp.util.buf_versions[buf],
      insert_enter = vim.fn.maparg("<CR>", "i", false, true),
      normal_enter = vim.fn.maparg("<CR>", "n", false, true),
      paste = vim.o.paste,
      formatoptions = vim.bo[buf].formatoptions,
      autocmds = autocmds,
    }
    record.sequence = nil
    write_probe(record)
  end
  vim.on_key(function(key)
    if key == "\r" or key == "\n" then
      probe("native-key", key)
    end
  end)
  vim.api.nvim_create_autocmd(
    { "TextChanged", "TextChangedI", "InsertEnter", "InsertLeave", "ModeChanged" },
    {
      callback = function(event)
        probe(event.event)
      end,
    }
  )
end
vim.lsp.handlers["view/render"] = function()
  vim.g.plain_smoke_rich_notifications = vim.g.plain_smoke_rich_notifications + 1
end

vim.api.nvim_create_autocmd("LspAttach", {
  callback = function(args)
    vim.g.plain_smoke_client_id = args.data.client_id
  end,
})

-- The standalone smoke deliberately keeps its original declarations and setup.
if vim.env.MAGIT_LSP_UI_CLIENT == "plain" then
  dofile(vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":h") .. "/plain_scenario.lua")
end
