--- Theme links for every server token type and modifier.
---
--- The server ships a suggested highlight group per token type in its
--- semantic-token legend (`tokenTypeGroups`), so a new token type needs no
--- client edit. This module only keeps client-local overrides and links the
--- modifiers, whose groups are conventional.

local M = {}

--- Client-only overrides, token type to theme group.
---
--- The server's suggestion wins for every type absent here, so this table is
--- only for local preferences or a temporarily bad server suggestion.
M.OVERRIDES = {}

--- Modifier groups are applied above the type, so they should only add
--- attributes (or a background) and never replace a type's foreground.
M.MODIFIER_LINKS = {
  -- keep-for: TASKS §3.15 recorded modifier styling; selected is used by log selection, remaining links support planned/ref presentation (R-5.3).
  staged = 'Bold',
  selected = 'Visual',
  planned = 'Comment',
  disabled = 'Comment',
  bold = 'Bold',
}

--- The legend of the most recently initialized client, kept for `:colorscheme`.
---@type table|nil
M.legend = nil

--- The client filetype whose `@lsp.type.*` groups this module defines.
---@type string|nil
local filetype

--- Link every semantic-token group of the current legend.
--- `default = true` keeps a user's own definition; `:colorscheme` clears the
--- groups, so this runs again on `ColorScheme`.
function M.define()
  local ft = filetype
  if not ft then
    return
  end
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

  local legend = M.legend or {}
  local suggested = legend.tokenTypeGroups or {}
  for _, token_type in ipairs(legend.tokenTypes or {}) do
    local group = M.OVERRIDES[token_type] or suggested[token_type]
    if group then
      local name = string.format('@lsp.type.%s.%s', token_type, ft)
      vim.api.nvim_set_hl(0, name, { link = group, default = true })
    end
  end
  for modifier, link in pairs(M.MODIFIER_LINKS) do
    local group = string.format('@lsp.mod.%s.%s', modifier, ft)
    vim.api.nvim_set_hl(0, group, { link = link, default = true })
  end
end

--- Remember the server legend and define its token-type links.
---@param ft string client filetype
---@param legend table|nil `semanticTokensProvider.legend` from `initialize`
function M.apply_legend(ft, legend)
  filetype = ft
  M.legend = legend
  M.define()
end

--- Define the links now and after every `:colorscheme`.
---@param ft string client filetype
function M.setup(ft)
  filetype = ft
  M.define()
  vim.api.nvim_create_autocmd('ColorScheme', {
    group = vim.api.nvim_create_augroup('magit_lsp_highlights', { clear = true }),
    callback = M.define,
  })
end

return M
