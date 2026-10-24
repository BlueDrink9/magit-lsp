--- Theme links for every server token type and modifier.
---
--- The server names token types; the client only links them to the
--- colourscheme's own groups, so no colour is hard-coded here.

local M = {}

--- Standard theme groups for every token type of TASKS.md §3.15.
M.TOKEN_LINKS = {
  ['magit.section'] = 'Title',
  ['magit.count'] = 'Comment',
  ['magit.file'] = 'Directory',
  ['magit.file.modified'] = 'Changed',
  ['magit.file.added'] = 'Added',
  ['magit.file.deleted'] = 'Removed',
  ['magit.file.renamed'] = 'Changed',
  ['magit.file.untracked'] = 'Comment',
  ['magit.hunk.header'] = 'Special',
  ['magit.diff.added'] = '@diff.plus',
  ['magit.diff.removed'] = '@diff.minus',
  ['magit.diff.context'] = 'Normal',
  ['magit.hash'] = 'Constant',
  ['magit.ref.local'] = 'Type',
  ['magit.ref.remote'] = 'Special',
  ['magit.ref.tag'] = 'Label',
  ['magit.ref.head'] = 'Type',
  ['magit.author'] = 'Identifier',
  ['magit.date'] = 'Comment',
  ['magit.message.warn'] = 'WarningMsg',
  ['magit.message.error'] = 'ErrorMsg',
  ['magit.menu.title'] = 'Title',
  ['magit.menu.group'] = 'Statement',
  ['magit.menu.key'] = 'Keyword',
  ['magit.menu.label'] = 'Normal',
  ['magit.menu.flag'] = 'Special',
  ['magit.menu.flag.off'] = 'MagitMenuFlagOff',
  ['magit.menu.flag.on'] = 'Special',
  ['magit.menu.switch.on'] = 'String',
  ['magit.menu.switch.off'] = 'Comment',
  ['magit.menu.option'] = 'String',
  ['magit.menu.value'] = 'String',
  ['magit.menu.planned'] = 'Comment',
  ['magit.menu.disabled'] = 'MagitMenuDisabled',
  ['magit.menu.pending'] = 'MagitMenuPending',
  ['magit.menu.preview'] = 'Comment',
}

--- Modifier groups are applied above the type, so they should only add
--- attributes (or a background) and never replace a type's foreground.
M.MODIFIER_LINKS = {
  staged = 'Bold',
  selected = 'Visual',
  planned = 'Comment',
  disabled = 'Comment',
  bold = 'Bold',
}

--- Link every semantic-token group of `filetype` to a standard theme group.
--- `default = true` keeps a user's own definition; `:colorscheme` clears the
--- groups, so this runs again on `ColorScheme`.
---@param filetype string
function M.define(filetype)
  -- A link cannot add attributes, so the off flag's dim is carried by an
  -- intermediate group; its colour still comes from the colourscheme.
  local comment = vim.api.nvim_get_hl(0, { name = 'Comment' })
  vim.api.nvim_set_hl(0, 'MagitMenuFlagOff', {
    fg = comment.fg,
    bg = comment.bg,
    dim = true,
  })
  -- A menu item that cannot complete the pending prefix, and the prefix of one
  -- that can. Both need an intermediate group because a link cannot add the
  -- dim/bold attributes.
  vim.api.nvim_set_hl(0, 'MagitMenuDisabled', {
    fg = comment.fg,
    bg = comment.bg,
    dim = true,
  })
  local keyword = vim.api.nvim_get_hl(0, { name = 'Keyword' })
  vim.api.nvim_set_hl(0, 'MagitMenuPending', {
    fg = keyword.fg,
    bg = keyword.bg,
    bold = true,
  })
  for token_type, link in pairs(M.TOKEN_LINKS) do
    local group = string.format('@lsp.type.%s.%s', token_type, filetype)
    vim.api.nvim_set_hl(0, group, { link = link, default = true })
  end
  for modifier, link in pairs(M.MODIFIER_LINKS) do
    local group = string.format('@lsp.mod.%s.%s', modifier, filetype)
    vim.api.nvim_set_hl(0, group, { link = link, default = true })
  end
end

--- Define the links now and after every `:colorscheme`.
---@param filetype string
function M.setup(filetype)
  M.define(filetype)
  vim.api.nvim_create_autocmd('ColorScheme', {
    group = vim.api.nvim_create_augroup('magit_lsp_highlights', { clear = true }),
    callback = function()
      M.define(filetype)
    end,
  })
end

return M
