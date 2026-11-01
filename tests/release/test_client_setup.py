"""Isolated real-server smoke for the documented on-demand Neovim snippets."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class ClientSetupTests(unittest.TestCase):
    """Run native startup and lazy-spec callbacks without production test hooks."""

    def test_documented_setup(self):
        nvim = shutil.which('nvim')
        binary = os.environ.get('MAGIT_LSP_TEST_BINARY')
        if not nvim or not binary:
            self.skipTest('requires nvim 0.11+ and MAGIT_LSP_TEST_BINARY')
        binary = (ROOT / binary).resolve()
        self.assertTrue(binary.is_file(), str(binary))
        document = (ROOT / 'docs/CLIENT_INSTALL.md').read_text()
        snippets = re.findall(r'```lua\n(.*?)\n```', document, re.S)
        self.assertEqual(len(snippets), 2)
        for mode, snippet in zip(('native-pack', 'lazy-spec-callback'), snippets):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(prefix='magit-setup-') as temporary:
                base = Path(temporary)
                home = base / 'home'
                server = home / '.local/bin/magit-lsp'
                server.parent.mkdir(parents=True)
                server.symlink_to(binary)
                package = base / 'data/nvim/site/pack/magit/start/magit-lsp'
                package.mkdir(parents=True)
                shutil.copytree(ROOT / 'lua', package / 'lua')
                repo = base / 'repo'
                repo.mkdir()
                subprocess.run(['git', 'init', '-q', str(repo)], check=True)
                (repo / 'new.txt').write_text('setup smoke\n')
                init = base / 'init.lua'
                spec = base / 'spec.lua'
                spec.write_text(snippet)
                init.write_text(snippet if mode == 'native-pack' else '-- no startup setup\n')
                action = "vim.cmd('MagitStatus')" if mode == 'native-pack' else "spec.keys[1][2]()"
                check = base / 'check.lua'
                check.write_text('''local ok, err = xpcall(function()
  assert(vim.fn.has('nvim-0.11') == 1, 'requires Neovim 0.11+')
  assert(package.loaded['magit-lsp'] == nil, 'startup loaded client')
  local spec
  if vim.env.SETUP_MODE == 'lazy-spec-callback' then
    spec = dofile(vim.env.SETUP_SPEC)
    assert(spec.lazy and spec.opts == nil and spec.config == nil)
    assert(package.loaded['magit-lsp'] == nil, 'spec opened status during loading')
  end
  assert(#vim.lsp.get_clients() == 0, 'startup spawned a server')
  ''' + action + '''
  assert(vim.wait(15000, function()
    if vim.bo.filetype ~= 'magit-lsp' then return false end
    for _, line in ipairs(vim.api.nvim_buf_get_lines(0, 0, -1, false)) do
      if line:find('new.txt', 1, true) then return true end
    end
    return false
  end, 20), 'status did not render new.txt')
end, debug.traceback)
for _, client in ipairs(vim.lsp.get_clients()) do client:stop() end
vim.wait(3000, function() return #vim.lsp.get_clients() == 0 end, 20)
if not ok then
  io.stderr:write(err .. '\\n')
  vim.cmd('cquit 1')
else
  print('setup smoke: ' .. vim.env.SETUP_MODE .. ' rendered new.txt')
  vim.cmd('qa!')
end
''')
                env = dict(os.environ, HOME=str(home), XDG_DATA_HOME=str(base / 'data'),
                           XDG_CONFIG_HOME=str(base / 'config'), XDG_STATE_HOME=str(base / 'state'),
                           XDG_CACHE_HOME=str(base / 'cache'), NVIM_APPNAME='nvim',
                           SETUP_MODE=mode, SETUP_SPEC=str(spec), SETUP_CHECK=str(check))
                env.pop('MAGIT_LSP_BIN', None)
                result = subprocess.run(
                    [nvim, '--headless', '-i', 'NONE', '-u', str(init),
                     '-c', "lua dofile(vim.env.SETUP_CHECK)"],
                    cwd=repo, env=env, text=True, capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn('setup smoke: ' + mode, result.stdout + result.stderr)
