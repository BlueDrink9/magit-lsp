"""Isolated real-server smoke for the documented nested Neovim client snippets."""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import unittest
from importlib.machinery import SourceFileLoader


ROOT = Path(__file__).resolve().parents[2]


def server_binary() -> Path | None:
    """Resolve the server binary under test, or None when none is built."""
    binary = os.environ.get('MAGIT_LSP_TEST_BINARY')
    if not binary:
        for candidate in ('target/debug/magit-lsp', 'target/release/magit-lsp'):
            if (ROOT / candidate).is_file():
                binary = candidate
                break
    return (ROOT / binary).resolve() if binary else None


def load_launcher() -> object:
    """Import the extensionless interactive launcher script as a module."""
    path = ROOT / 'scripts/neovim'
    loader = SourceFileLoader('magit_neovim_launcher_smoke', str(path))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    if spec is None or spec.loader is None:
        raise RuntimeError(f'cannot import {path}')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def owned_pids(launch_dir: Path) -> list[str]:
    """Live PIDs whose environment still names `launch_dir` (Linux only)."""
    proc = Path('/proc')
    if not proc.is_dir():
        return []
    token = f'MAGIT_LSP_LAUNCH_DIR={launch_dir}'.encode()
    found = []
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            environ = (entry / 'environ').read_bytes()
        except OSError:
            continue
        if token in environ.split(b'\0'):
            found.append(entry.name)
    return found


class ClientSetupTests(unittest.TestCase):
    """Run native startup and lazy-spec callbacks without production test hooks."""

    def test_documented_setup(self):
        nvim = shutil.which('nvim')
        binary = server_binary()
        if not nvim or not binary:
            self.skipTest('requires nvim 0.11+ and a magit-lsp binary')
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


class LauncherSmokeTests(unittest.TestCase):
    """Start the real interactive launcher argv in a temporary fixture.

    The adapter adds only `--headless` and a check script to the launcher's
    generated argv: the environment, dedicated initializer, client runtime and
    server are the ones `make neovim` uses.
    """

    CHECK = '''local ok, err = xpcall(function()
  assert(vim.fn.has('nvim-0.11') == 1, 'requires Neovim 0.11+')
  local shada = assert(vim.env.MAGIT_LSP_SMOKE_SHADA,
    'smoke adapter did not name the ShaDa path')
  assert(vim.fn.isdirectory(shada) == 0,
    'ShaDa path is a directory, not a file: ' .. shada)
  assert(vim.wait(20000, function()
    if vim.bo.filetype ~= 'magit-lsp' then return false end
    for _, line in ipairs(vim.api.nvim_buf_get_lines(0, 0, -1, false)) do
      if line:find('new.txt', 1, true) then return true end
    end
    return false
  end, 20), 'launcher did not render new.txt in a status view')
  assert(#vim.lsp.get_clients() == 1, 'launcher did not start exactly one server')
  vim.cmd('wshada')
  assert(vim.fn.filereadable(shada) == 1, 'ShaDa file was not written: ' .. shada)
  for _, client in ipairs(vim.lsp.get_clients()) do client:stop() end
  vim.wait(5000, function() return #vim.lsp.get_clients() == 0 end, 20)
end, debug.traceback)
if not ok then
  io.stderr:write(err .. '\\n')
  vim.cmd('cquit 1')
else
  print('launcher smoke: rendered status and wrote shada')
  vim.cmd('qa!')
end
'''

    def setUp(self) -> None:
        self.nvim = shutil.which('nvim')
        self.binary = server_binary()
        if not self.nvim:
            self.skipTest('requires nvim 0.11+')
        if not self.binary or not self.binary.is_file():
            self.skipTest('requires a magit-lsp binary')
        self.launcher = load_launcher()
        self.root = Path(tempfile.mkdtemp(prefix='magit launcher smoke-'))
        self.addCleanup(shutil.rmtree, self.root, True)

    def run_smoke(self, *, directory_shada: bool = False):
        """Launch the real argv and return its status, observations and repo."""
        repo = self.root / 'repo with spaces'
        repo.mkdir()
        subprocess.run(['git', 'init', '-q', str(repo)], check=True)
        (repo / 'new.txt').write_text('launcher smoke\n')
        temporary_root = self.root / 'temporary root with spaces'
        temporary_root.mkdir()
        check = self.root / 'check.lua'
        check.write_text(self.CHECK)
        observed: dict[str, object] = {}

        def adapter(command, **options):
            argv = list(command)
            environment = dict(options['env'])
            base = Path(environment['MAGIT_LSP_LAUNCH_DIR'])
            shada_index = argv.index('-i') + 1
            shada = Path(argv[shada_index])
            observed.update({
                'argv': argv,
                'init': Path(argv[argv.index('-u') + 1]),
                'base': base,
                'base_existed': base.is_dir(),
                'shada_parent_existed': shada.parent.is_dir(),
                'shada_is_directory': shada.is_dir(),
            })
            if directory_shada:
                # The superseded launcher passed the isolated directory itself.
                argv[shada_index] = str(shada.parent)
            environment.update({
                'MAGIT_LSP_SMOKE_SHADA': argv[shada_index],
                'MAGIT_LSP_SMOKE_CHECK': str(check),
            })
            process = subprocess.Popen(
                argv + ['--headless', '-c',
                        'lua dofile(vim.env.MAGIT_LSP_SMOKE_CHECK)'],
                cwd=options['cwd'], env=environment, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            # The editor and its server inherit the launcher environment, so
            # the detector must see them while the editor is still running.
            observed['owned_running'] = len(owned_pids(base))
            stdout, stderr = process.communicate(timeout=90)
            result = subprocess.CompletedProcess(
                argv, process.returncode, stdout, stderr)
            observed['result'] = result
            observed['shada_written'] = (
                shada.is_file() and shada.stat().st_size > 0)
            return result

        status = self.launcher.launch(  # type: ignore[attr-defined]
            repo, nvim=self.nvim, server=self.binary, runner=adapter,
            temporary_root=temporary_root)
        return status, observed, repo

    def assert_no_leaked_processes(self, base: Path) -> None:
        """Fail while a launched descendant still names the removed launch dir."""
        deadline = time.monotonic() + 10
        leaked = owned_pids(base)
        while leaked and time.monotonic() < deadline:
            time.sleep(0.05)
            leaked = owned_pids(base)
        self.assertEqual(
            leaked, [], f'launched processes outlived the editor: {leaked}')

    def test_launcher_smoke_renders_writes_shada_and_cleans_up(self) -> None:
        status, observed, repo = self.run_smoke()
        result = observed['result']
        output = result.stdout + result.stderr
        self.assertEqual(status, 0, output)
        self.assertEqual(result.returncode, 0, output)
        self.assertIn('launcher smoke: rendered status and wrote shada', output)
        self.assertNotRegex(result.stderr, r'(?m)^Error|E\d{3,4}:')
        self.assertEqual(observed['init'], self.launcher.INIT)  # type: ignore[attr-defined]
        self.assertNotIn('minimal_init', ' '.join(observed['argv']))
        self.assertIn('--noplugin', observed['argv'])
        # Both the fixture and the temporary launch root contain a space.
        self.assertIn(' ', str(observed['base']))
        self.assertIn(' ', str(repo))
        self.assertTrue(observed['base_existed'])
        self.assertTrue(observed['shada_parent_existed'])
        self.assertFalse(observed['shada_is_directory'])
        self.assertTrue(observed['shada_written'])
        self.assertGreaterEqual(
            observed['owned_running'], 1,
            'owned-process detector never saw the launched editor')
        self.assertFalse(
            Path(observed['base']).exists(), 'launch directory was not removed')
        self.assert_no_leaked_processes(Path(observed['base']))
        print('launcher smoke: dedicated initializer started a real status '
              'view with spaces, wrote ShaDa and removed the launch directory')

    def test_launcher_smoke_rejects_a_directory_as_the_shada_file(self) -> None:
        status, observed, _ = self.run_smoke(directory_shada=True)
        result = observed['result']
        output = result.stdout + result.stderr
        self.assertNotEqual(status, 0, output)
        self.assertNotEqual(result.returncode, 0, output)
        self.assertNotIn('launcher smoke: rendered status and wrote shada', output)
        self.assertRegex(output, r'(?i)shada')
        self.assertFalse(
            Path(observed['base']).exists(), 'launch directory was not removed')
        failure = next(
            (line.strip() for line in output.splitlines()
             if 'shada' in line.lower()), '')
        print(f'launcher smoke rejected directory ShaDa argv: {failure}')
