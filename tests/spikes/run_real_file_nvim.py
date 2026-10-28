#!/usr/bin/env python3
"""Drive only an owned tmux/Neovim UI and retain real-file LSP spike evidence."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import time


def main():
    """Run the native close gate plus dirty/template/language investigations."""
    root = Path(__file__).resolve().parents[2]
    base = Path(tempfile.mkdtemp(prefix='real-file-nvim-', dir='/tmp/opencode'))
    repo = base / 'repo'
    subprocess.run(['git', 'init', '-q', str(repo)], check=True)
    target = repo / '.git/COMMIT_EDITMSG'
    target.write_text('spike message\n')
    socket = str(base / 'nvim.sock')
    tmux = ['tmux', '-L', base.name]
    env = {**os.environ, 'MAGIT_REAL_FILE_ROOT': str(root), 'MAGIT_REAL_FILE_EVIDENCE': str(base), 'MAGIT_REAL_FILE_TARGET': str(target)}
    command = ['nvim', '-u', str(root / 'tests/spikes/real_file_nvim.lua'), '--listen', socket]
    result = {'gate': 'incomplete', 'cases': {}, 'command': command,
              'version': subprocess.check_output(['nvim', '--version'], text=True)}

    def rpc(expression):
        return subprocess.check_output(['nvim', '--server', socket, '--remote-expr', expression], text=True, timeout=5).strip()

    def state():
        return json.loads(rpc('v:lua.SpikeState()'))

    def wait(check, label, timeout=5):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if check():
                return
        raise RuntimeError('Deadline: ' + label)

    def wire():
        path = base / 'wire.jsonl'
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def count(method):
        return sum(m.get('method') == method for m in wire())

    def keys(*values):
        subprocess.run(tmux + ['send-keys', '-t', 'spike:0.0', *values], check=True)

    def screen(name):
        (base / (name + '.screen')).write_text(subprocess.check_output(tmux + ['capture-pane', '-p', '-t', 'spike:0.0'], text=True))

    try:
        subprocess.run(tmux + ['new-session', '-d', '-s', 'spike', '-x', '100', '-y', '30', shlex.join(command)], env=env, check=True)
        wait(lambda: Path(socket).exists(), 'RPC socket')
        wait(lambda: state()['attached'] and count('textDocument/didOpen') == 1, 'actual didOpen')
        result['cases']['open'] = state()
        screen('01-open')
        keys('i', 'dirty native text', 'Escape')
        wait(lambda: state()['modified'], 'dirty input')
        keys(':q', 'Enter')
        wait(lambda: 'E37' in state()['error'], 'native dirty quit refusal')
        result['cases']['dirtyQuit'] = state()
        screen('02-dirty-refused')
        keys(':w', 'Enter')
        wait(lambda: count('textDocument/didSave') == 1 and not state()['modified'], 'native save')
        result['cases']['savedDisk'] = target.read_text()
        target.write_text('replacement template\n')
        rpc('luaeval("SpikeShow()")')
        result['cases']['reusedShow'] = state()
        rpc('luaeval("SpikeRefresh()")')
        wait(lambda: state()['text'] == 'server refreshed template', 'applyEdit refresh')
        result['cases']['applyEdit'] = state()
        before = (count('textDocument/didOpen'), count('textDocument/didClose'))
        rpc('execute("set filetype=text")')
        wait(lambda: state()['language'] == 'text', 'language mode')
        result['cases']['languageChange'] = {'state': state(), 'openDelta': count('textDocument/didOpen') - before[0], 'closeDelta': count('textDocument/didClose') - before[1]}
        screen('03-refresh-language')
        keys(':w', 'Enter')
        wait(lambda: 'Do you really want to write' in subprocess.check_output(tmux + ['capture-pane', '-p', '-t', 'spike:0.0'], text=True), 'native external-change write warning')
        screen('03b-external-change-warning')
        keys('y')
        wait(lambda: 'Press ENTER' in subprocess.check_output(tmux + ['capture-pane', '-p', '-t', 'spike:0.0'], text=True), 'write acknowledgement')
        keys('Enter')
        wait(lambda: not state()['modified'], 'save refreshed template')
        before = count('textDocument/didClose')
        started = time.monotonic()
        keys(':q', 'Enter')
        wait(lambda: count('textDocument/didClose') > before, 'native close didClose')
        result['cases']['nativeClose'] = {'elapsedMs': round((time.monotonic() - started) * 1000), 'state': state(), 'wireCloseDelta': count('textDocument/didClose') - before}
        screen('04-native-close')
        result['gate'] = 'pass'
    except Exception as error:
        result['error'] = repr(error)
        try:
            screen('failure')
        except Exception:
            pass
    finally:
        (base / 'result.json').write_text(json.dumps(result, indent=2))
        subprocess.run(tmux + ['kill-server'], capture_output=True)
    print('Evidence:', base)
    print(json.dumps(result, indent=2))
    return 0 if result['gate'] == 'pass' else 1


if __name__ == '__main__':
    raise SystemExit(main())
