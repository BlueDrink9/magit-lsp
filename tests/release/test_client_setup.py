"""Isolated real-server smoke for the documented nested Neovim client snippets."""
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
        if not binary:
            for candidate in ('target/debug/magit-lsp', 'target/release/magit-lsp'):
                if (ROOT / candidate).is_file():
                    binary = candidate
                    break
        if not nvim or not binary:
            self.skipTest('requires nvim 0.11+ and a magit-lsp binary')
        binary = (ROOT / binary).resolve()
        self.assertTrue(binary.is_file(), str(binary))
        document = (ROOT / 'docs/CLIENT_INSTALL.md').read_text()
        snippets = {}
        for mode, marker in (
                ('native-pack', '-- native-pack init.lua'),
                ('lazy-spec-callback', '-- lazy.nvim plugin spec')):
            match = re.search(
                r'```lua\n(' + re.escape(marker) + r'\n.*?)\n```', document, re.S)
            self.assertIsNotNone(
                match, f'docs/CLIENT_INSTALL.md has no {mode} snippet {marker!r}')
            snippets[mode] = match.group(1)
        for mode, snippet in snippets.items():
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(prefix='magit-setup-') as temporary:
                base = Path(temporary)
                home = base / 'home'
                server = home / '.local/bin/magit-lsp'
                server.parent.mkdir(parents=True)
                server.symlink_to(binary)
                if mode == 'native-pack':
                    checkout = base / 'data/nvim/site/pack/magit/start/magit-lsp'
                else:
                    checkout = base / 'checkout'
                checkout.mkdir(parents=True)
                # The checkout ships the Neovim client only under clients/neovim;
                # copying a root lua/ or plugin/ would fabricate a compatibility
                # runtime the product no longer provides.
                shutil.copytree(ROOT / 'clients/neovim', checkout / 'clients/neovim')
                self.assertFalse((checkout / 'lua').exists(), str(checkout))
                self.assertFalse((checkout / 'plugin').exists(), str(checkout))
                repo = base / 'repo'
                repo.mkdir()
                subprocess.run(['git', 'init', '-q', str(repo)], check=True)
                (repo / 'new.txt').write_text('setup smoke\n')
                init = base / 'init.lua'
                spec = base / 'spec.lua'
                spec.write_text(snippet)
                init.write_text(snippet if mode == 'native-pack' else '-- no startup setup\n')
                check = base / 'check.lua'
                check.write_text('''local ok, err = xpcall(function()
  assert(vim.fn.has('nvim-0.11') == 1, 'requires Neovim 0.11+')
  local checkout = vim.env.SETUP_CHECKOUT
  assert(vim.fn.isdirectory(checkout .. '/clients/neovim/lua') == 1,
    'fixture has no clients/neovim runtime')
  assert(vim.fn.isdirectory(checkout .. '/lua') == 0
    and vim.fn.isdirectory(checkout .. '/plugin') == 0,
    'fixture fabricates a root compatibility runtime')
  local function status_buffers()
    local count = 0
    for _, buf in ipairs(vim.api.nvim_list_bufs()) do
      if vim.bo[buf].filetype == 'magit-lsp' then count = count + 1 end
    end
    return count
  end
  assert(package.loaded['magit-lsp'] == nil, 'startup loaded client')
  assert(#vim.lsp.get_clients() == 0, 'startup spawned a server')
  assert(status_buffers() == 0, 'startup opened a status view')
  local spec
  local function trigger()
    if vim.env.SETUP_MODE == 'native-pack' then
      vim.cmd('MagitStatus')
    else
      spec.keys[1][2]()
    end
  end
  if vim.env.SETUP_MODE == 'native-pack' then
    assert(vim.fn.exists(':MagitStatus') == 2, 'documented init did not define :MagitStatus')
  else
    spec = dofile(vim.env.SETUP_SPEC)
    assert(spec.lazy and spec.opts == nil and type(spec.config) == 'function')
    assert(#vim.lsp.get_clients() == 0, 'spec loading started a server')
    assert(not pcall(require, 'magit-lsp'), 'spec loading resolved the client module')
    -- lazy.nvim adds the plugin root; only the config hook may add the client.
    vim.opt.rtp:append(checkout)
    assert(vim.fn.exists(':Magit') == 0, 'plugin root on runtimepath registered :Magit')
    assert(not pcall(require, 'magit-lsp'), 'plugin root on runtimepath resolved the client')
    spec.config({ dir = checkout })
    assert(vim.fn.exists(':Magit') == 2, 'lazy config did not source the plugin entry point')
    assert(package.loaded['magit-lsp'] == nil, 'lazy config loaded the client module')
    assert(#vim.lsp.get_clients() == 0, 'lazy config started a server')
    assert(status_buffers() == 0, 'lazy config opened a status view')
  end
  trigger()
  assert(vim.wait(15000, function()
    if vim.bo.filetype ~= 'magit-lsp' then return false end
    for _, line in ipairs(vim.api.nvim_buf_get_lines(0, 0, -1, false)) do
      if line:find('new.txt', 1, true) then return true end
    end
    return false
  end, 20), 'status did not render new.txt')
  local buffer = vim.api.nvim_get_current_buf()
  local source = debug.getinfo(require('magit-lsp').setup, 'S').source:gsub('^@', '')
  assert(source:find('/clients/neovim/lua/magit%-lsp/init%.lua$'),
    'client module loaded outside clients/neovim: ' .. source)
  assert(#vim.lsp.get_clients() == 1, 'trigger did not start exactly one server')
  -- The documented repeat trigger focuses the existing rendered status view.
  trigger()
  assert(vim.wait(5000, function()
    return vim.api.nvim_get_current_buf() == buffer
  end, 20), 'repeat trigger did not focus the rendered status')
  assert(status_buffers() == 1, 'repeat trigger opened a second status buffer')
  assert(#vim.lsp.get_clients() == 1, 'repeat trigger started a second server')
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
                           SETUP_MODE=mode, SETUP_SPEC=str(spec), SETUP_CHECK=str(check),
                           SETUP_CHECKOUT=str(checkout))
                env.pop('MAGIT_LSP_BIN', None)
                result = subprocess.run(
                    [nvim, '--headless', '-i', 'NONE', '-u', str(init),
                     '-c', "lua dofile(vim.env.SETUP_CHECK)"],
                    cwd=repo, env=env, text=True, capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn('setup smoke: ' + mode, result.stdout + result.stderr)
                print(f'setup smoke: {mode} started and focused the rendered status')

    def test_documented_lazy_spec_contract(self):
        """The documented lazy spec is valid Lua with no opts/build and a runtime hook."""
        document = (ROOT / 'docs/CLIENT_INSTALL.md').read_text()
        match = re.search(
            r'```lua\n(-- lazy\.nvim plugin spec\n.*?)\n```', document, re.S)
        self.assertIsNotNone(
            match, 'docs/CLIENT_INSTALL.md has no lazy.nvim plugin spec block')
        snippet = match.group(1)
        for key in ('opts', 'build'):
            self.assertNotRegex(
                snippet, rf'\b{key}\s*=',
                f'documented lazy spec must not set {key}')
        nvim = shutil.which('nvim')
        if not nvim:
            return
        with tempfile.TemporaryDirectory(prefix='magit-lazy-spec-') as temporary:
            base = Path(temporary)
            spec = base / 'spec.lua'
            spec.write_text(snippet)
            check = base / 'check.lua'
            check.write_text('''local ok, err = xpcall(function()
  local chunk = assert(loadfile(vim.env.SETUP_SPEC))
  local spec = chunk()
  assert(spec.lazy, 'documented lazy spec must be lazy')
  assert(spec.opts == nil, 'documented lazy spec must not set opts')
  assert(type(spec.config) == 'function', 'documented lazy spec must add the client runtime')
  assert(spec.build == nil, 'documented lazy spec must not set build')
end, debug.traceback)
if not ok then
  io.stderr:write(err .. '\\n')
  vim.cmd('cquit 1')
else
  print('lazy spec contract: ok')
  vim.cmd('qa!')
end
''')
            result = subprocess.run(
                [nvim, '--headless', '-u', 'NONE', '-i', 'NONE',
                 '-c', 'lua dofile(vim.env.SETUP_CHECK)'],
                env=dict(os.environ, SETUP_SPEC=str(spec), SETUP_CHECK=str(check)),
                text=True, capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('lazy spec contract: ok', result.stdout + result.stderr)
