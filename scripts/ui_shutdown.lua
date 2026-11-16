--- Stop native LSP clients and reap the owned server before exiting Neovim.
return function(pid)
  for _, client in ipairs(vim.lsp.get_clients()) do
    client:stop()
  end
  local exited = function() return vim.uv.kill(pid, 0) == nil end
  if not vim.wait(1000, exited, 20) then
    vim.uv.kill(pid, 15)
    if not vim.wait(1000, exited, 20) then
      vim.uv.kill(pid, 9)
      assert(vim.wait(1000, exited, 20), 'server child was not reaped')
    end
  end
  vim.cmd('qa!')
end
