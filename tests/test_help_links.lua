local MiniTest = require("mini.test")
local child = MiniTest.new_child_neovim()

local here = vim.fs.dirname(debug.getinfo(1, "S").source:sub(2))
local project = vim.fs.dirname(here)

--- Documents the plugin ships next to `doc/magit.txt`; every one of them is
--- reachable from the help text without the network.
local SHIPPED = {
  "README.md",
  "docs/CLIENT_INSTALL.md",
  "docs/KEYMAPS.md",
  "docs/PROTOCOL.md",
}

local T = MiniTest.new_set({
  hooks = {
    pre_case = function()
      child.restart({ "-u", "scripts/minimal_init.lua" })
    end,
    post_case = function()
      child.stop()
    end,
  },
})

--- Every relative Markdown path the help text mentions, in order of appearance.
local function help_references()
  local text = table.concat(vim.fn.readfile(project .. "/doc/magit.txt"), "\n")
  local refs, seen = {}, {}
  for token in text:gmatch("[^%s`|]+") do
    local ref = token:match("^(%.%./[%w%./_-]+%.md)")
    if ref and not seen[ref] then
      seen[ref] = true
      refs[#refs + 1] = ref
    end
  end
  return refs
end

T["help points at shipped documents by relative local path"] = function()
  local text = table.concat(vim.fn.readfile(project .. "/doc/magit.txt"), "\n")
  for _, rel in ipairs(SHIPPED) do
    MiniTest.expect.equality(
      text:find("../" .. rel, 1, true) ~= nil,
      true,
      "doc/magit.txt refers to ../" .. rel
    )
    MiniTest.expect.equality(
      vim.fn.filereadable(project .. "/" .. rel),
      1,
      rel .. " ships with the plugin"
    )
  end
  MiniTest.expect.equality(
    text:find("github.com/BlueDrink9/magit-lsp/blob/", 1, true),
    nil,
    "locally shipped documents are not linked through GitHub blob URLs"
  )
  -- The rest of the help file is unchanged in substance: commands, setup,
  -- keys and troubleshooting stay documented.
  for _, needle in ipairs({ "*magit*", ":Magit [directory]", "$MAGIT_LSP_BIN", "protocol mismatch" }) do
    MiniTest.expect.equality(
      text:find(needle, 1, true) ~= nil,
      true,
      "help still documents " .. needle
    )
  end
end

T["gf opens every shipped document from the help buffer in an unrelated cwd"] = function()
  local refs = help_references()
  MiniTest.expect.equality(#refs > 0, true, "doc/magit.txt references local documents")
  for _, ref in ipairs(refs) do
    local rel = ref:sub(4)
    MiniTest.expect.equality(
      vim.fn.filereadable(project .. "/" .. rel),
      1,
      ref .. " is a real file in the plugin checkout"
    )
  end

  local help_root = vim.fn.tempname() .. "-help-links"
  local other_cwd = vim.fn.tempname() .. "-help-links-cwd"
  child.lua(string.format(
    [[
    local project, help_root, other_cwd = %q, %q, %q
    local refs = %s
    vim.fn.delete(help_root, 'rf')
    vim.fn.delete(other_cwd, 'rf')
    vim.fn.mkdir(help_root .. '/doc', 'p')
    vim.fn.mkdir(other_cwd, 'p')
    local function copy(rel)
      local dst = help_root .. '/' .. rel
      vim.fn.mkdir(vim.fs.dirname(dst), 'p')
      assert(vim.uv.fs_copyfile(project .. '/' .. rel, dst), 'copy ' .. rel)
    end
    copy('doc/magit.txt')
    for _, ref in ipairs(refs) do
      copy(ref:sub(4))
    end
    vim.cmd('cd ' .. vim.fn.fnameescape(other_cwd))
    local results = {}
    for _, ref in ipairs(refs) do
      vim.cmd('edit ' .. vim.fn.fnameescape(help_root .. '/doc/magit.txt'))
      local lnum, col
      for i, line in ipairs(vim.api.nvim_buf_get_lines(0, 0, -1, false)) do
        local s = line:find(ref, 1, true)
        if s then
          lnum, col = i, s - 1
          break
        end
      end
      local result = {
        ref = ref,
        cwd = vim.fn.getcwd(),
        source = vim.api.nvim_buf_get_name(0),
        source_filetype = vim.bo.filetype,
      }
      if not lnum then
        result.error = 'reference is not in the help text'
      else
        vim.api.nvim_win_set_cursor(0, { lnum, col })
        local ok, err = pcall(vim.cmd, 'normal! gf')
        result.ok = ok
        result.error = (not ok) and tostring(err) or nil
        result.bufname = vim.api.nvim_buf_get_name(0)
        result.buftype = vim.bo.buftype
        result.content_sha = vim.fn.sha256(
          table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n')
        )
      end
      results[#results + 1] = result
    end
    RESULTS = results
  ]],
    project,
    help_root,
    other_cwd,
    vim.inspect(refs)
  ))

  local results = child.lua_get("RESULTS")
  MiniTest.expect.equality(#results, #refs, "every help reference was navigated")
  for _, result in ipairs(results) do
    local ref = result.ref
    MiniTest.expect.equality(result.cwd, other_cwd, ref .. ": gf runs from an unrelated cwd")
    MiniTest.expect.equality(
      result.source,
      help_root .. "/doc/magit.txt",
      ref .. ": navigating from the shipped help file"
    )
    MiniTest.expect.equality(result.source_filetype, "help", ref .. ": source is a help buffer")
    MiniTest.expect.equality(result.error, nil, ref .. ": " .. tostring(result.error))
    MiniTest.expect.equality(result.ok, true, ref .. ": gf succeeded")
    MiniTest.expect.equality(result.buftype, "", ref .. ": opens a normal file buffer")
    MiniTest.expect.equality(
      result.bufname:find("://", 1, true),
      nil,
      ref .. ": no URL handler ran"
    )
    local rel = ref:sub(4)
    MiniTest.expect.equality(
      vim.uv.fs_realpath(result.bufname),
      vim.uv.fs_realpath(help_root .. "/" .. rel),
      ref .. ": gf lands on the shipped document"
    )
    MiniTest.expect.equality(
      result.content_sha,
      vim.fn.sha256(table.concat(vim.fn.readfile(project .. "/" .. rel), "\n")),
      ref .. ": the opened buffer holds the shipped document"
    )
  end

  child.lua(
    string.format(
      "vim.cmd('cd ' .. vim.fn.fnameescape(%q)); vim.fn.delete(%q, 'rf'); vim.fn.delete(%q, 'rf')",
      project,
      help_root,
      other_cwd
    )
  )
end

return T
