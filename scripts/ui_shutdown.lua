--- Stop native LSP clients and reap the owned server before exiting Neovim.
return function(pid)
  local clients = vim.lsp.get_clients()
  for _, client in ipairs(clients) do
    client:stop()
  end
  local exited = function() return vim.uv.kill(pid, 0) == nil end
  if not vim.wait(1000, exited, 20) then
    for _, client in ipairs(clients) do
      client:stop(true)
    end
    if not vim.wait(1000, exited, 20) then
      assert(vim.wait(1000, exited, 20), 'server child was not reaped')
    end
  end
  vim.cmd('qa!')
end
