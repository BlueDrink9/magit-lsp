import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, appendFileSync } from 'node:fs';
import { spawn, execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:net';

const root = resolve(fileURLToPath(new URL('../../../..', import.meta.url)));
const editor = process.env.MAGIT_VSCODE_BIN;
assert(editor && existsSync(editor) && process.env.DISPLAY && existsSync(`${root}/target/debug/magit-lsp`), 'MAGIT_VSCODE_BIN, DISPLAY and built server required');
execFileSync('import', ['-version']);
const directory = mkdtempSync('/tmp/opencode/real-file-lifecycle-');
console.log(`Evidence: ${directory}`);
const json = path => JSON.parse(readFileSync(path, 'utf8'));
const lines = path => existsSync(path) ? readFileSync(path, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
const write = (path, data) => writeFileSync(path, JSON.stringify(data, null, 2));
/** Poll observable state, surfacing durable host errors even after the host dies. */
async function wait(check, label, base, timeout = 90000) {
  const deadline = Date.now() + timeout;
  while (true) {
    if (base && existsSync(`${base}/failure.txt`)) throw Error(readFileSync(`${base}/failure.txt`, 'utf8'));
    const result = await check();
    if (result) return result;
    if (Date.now() > deadline) throw Error(`Deadline: ${label}`);
    await new Promise(resolve => setTimeout(resolve, 25));
  }
}
/** Linux process start times protect ownership checks against recycled PIDs. */
function processes() {
  return readdirSync('/proc').filter(p => /^\d+$/.test(p)).flatMap(p => {
    try {
      const stat = readFileSync(`/proc/${p}/stat`, 'utf8').split(') ')[1].split(' ');
      return [{ pid: Number(p), parent: Number(stat[1]), start: stat[19], state: stat[0], command: readFileSync(`/proc/${p}/cmdline`, 'utf8').replaceAll('\0', ' ') }];
    } catch { return []; }
  });
}
const cases = [];
try {
  for (const action of ['close-window', 'reload-window', 'workspace-change']) {
    const base = `${directory}/${action}`;
    const repo = `${base}/repo`;
    mkdirSync(repo, { recursive: true });
    const git = (...args) => execFileSync('git', ['-C', repo, ...args], { encoding: 'utf8' });
    git('init', '-q'); git('config', 'user.name', 'Test'); git('config', 'user.email', 'test@example.com');
    writeFileSync(`${repo}/tracked.txt`, 'initial\n'); git('add', '.'); git('commit', '-qm', 'initial');
    writeFileSync(`${repo}/tracked.txt`, `staged ${action}\n`); git('add', 'tracked.txt');
    writeFileSync(`${repo}/tracked.txt`, `unstaged ${action}\n`);
    const snapshot = () => ({ head: git('rev-parse', 'HEAD'), index: git('write-tree'), worktree: git('diff', '--binary'), status: git('status', '--porcelain') });
    const before = snapshot();
    mkdirSync(`${base}/other-workspace`); mkdirSync(`${base}/launcher`); mkdirSync(`${base}/user/User`, { recursive: true });
    write(`${base}/user/User/settings.json`, { 'magit-lsp.serverPath': `${root}/target/debug/magit-lsp`, 'window.title': base, 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'security.workspace.trust.enabled': false, 'git.enabled': false });
    const listener = createServer();
    await new Promise(r => listener.listen(0, '127.0.0.1', r));
    const port = listener.address().port;
    await new Promise(r => listener.close(r));
    write(`${base}/launcher/package.json`, { publisher: 'test', name: 'native-lifecycle', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './launcher.js', activationEvents: ['*'] });
    writeFileSync(`${base}/launcher/launcher.js`, `exports.activate = () => { setImmediate(() => require(${JSON.stringify(`${root}/clients/vscode/test/e2e/real-file-lifecycle.js`)}).run(${JSON.stringify({ root, evidence: base, repo, action, port })}).catch(e => require('fs').writeFileSync(${JSON.stringify(`${base}/failure.txt`)}, e.stack))); }; exports.deactivate = () => require('fs').appendFileSync(${JSON.stringify(`${base}/host.jsonl`)}, JSON.stringify({event:'deactivate',pid:process.pid,time:Date.now()})+'\\n');`);
    const env = Object.fromEntries(['HOME', 'PATH', 'DISPLAY', 'XAUTHORITY', 'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'LANG', 'USER', 'SHELL'].filter(k => process.env[k]).map(k => [k, process.env[k]]));
    const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust', `--remote-debugging-port=${port}`, '--user-data-dir', `${base}/user`, '--extensions-dir', `${base}/extensions`, `--extensionDevelopmentPath=${root}/clients/vscode`, `--extensionDevelopmentPath=${base}/launcher`, repo];
    write(`${base}/command.json`, { editor, args });
    const owned = new Map();
    const census = () => {
      const all = processes();
      let changed = true;
      while (changed) {
        changed = false;
        for (const p of all) if ((p.command.includes(`${base}/user`) || owned.has(p.parent)) && !owned.has(p.pid)) { owned.set(p.pid, p); changed = true; }
      }
      return all.filter(p => owned.get(p.pid)?.start === p.start && p.state !== 'Z');
    };
    let watcher;
    try {
      const child = spawn(editor, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
      child.stdout.on('data', d => appendFileSync(`${base}/stdout.txt`, d));
      child.stderr.on('data', d => appendFileSync(`${base}/stderr.txt`, d));
      child.on('error', e => writeFileSync(`${base}/failure.txt`, e.stack));
      child.on('exit', (code, signal) => write(`${base}/exit.json`, { code, signal }));
      watcher = setInterval(census, 25);
      await wait(() => existsSync(`${base}/ready.json`), 'saved production draft', base);
      const ready = json(`${base}/ready.json`);
      census();
      const pending = [...owned.values()];
      write(`${base}/processes-before.json`, pending);
      const server = pending.find(p => p.command.trim() === `${root}/target/debug/magit-lsp`);
      assert(server, 'owned production server observed');
      const descendants = new Set([server.pid]);
      for (let i = 0; i < pending.length; i++) for (const p of pending) if (descendants.has(p.parent)) descendants.add(p.pid);
      const helpers = pending.filter(p => descendants.has(p.pid) && p.pid !== server.pid);
      assert(helpers.length >= 2, 'pending Git and editor helper observed');
      assert.deepEqual(snapshot(), before);
      write(`${base}/go.json`, {});
      await wait(() => action === 'close-window' ? existsSync(`${base}/exit.json`) : existsSync(`${base}/resumed.json`), 'actual native lifecycle completed', base);
      if (action === 'close-window') {
        assert.equal(json(`${base}/exit.json`).code, 0);
        await wait(() => census().length === 0, 'native window and owned processes exited', base, 10000);
        execFileSync('import', ['-window', 'root', `${base}/after.png`]);
      } else {
        assert(json(`${base}/resumed.json`).workspace.includes(`file://${base}/${action === 'workspace-change' ? 'other-workspace' : 'repo'}`));
        if (action === 'reload-window') await wait(() => existsSync(`${base}/fresh.json`), 'fresh run rejects late confirmation', base);
      }
      await wait(() => !census().some(p => [server, ...helpers].some(old => p.pid === old.pid && p.start === old.start)), 'eventual server/helper cancellation', base, 10000);
      write(`${base}/processes-after.json`, census());
      const events = lines(`${base}/host.jsonl`);
      assert(events.some(e => e.event === 'deactivate' && e.pid === ready.hostPid), 'original production host deactivated');
      assert(events.some(e => e.event === 'progress' && e.pid === ready.hostPid && e.value.kind === 'begin'), 'pending production progress observed');
      const confirms = events.filter(e => e.method === 'workspace/executeCommand' && e.params.arguments?.[0]?.command === 'common.confirm');
      assert.equal(confirms.length, action === 'reload-window' ? 1 : 0, 'only deliberate stale probe, never lifecycle confirmation');
      assert.equal(confirms.filter(e => e.pid === ready.hostPid).length, 0, 'native lifecycle never confirms');
      assert.deepEqual(snapshot(), before, 'HEAD/index/worktree unchanged');
      if (action !== 'reload-window') assert.equal(readFileSync(new URL(ready.uri), 'utf8'), ready.text);
      assert(existsSync(new URL(ready.uri)), 'provider file preserved');
      // A later fresh-run cancellation can close the same URI in the new host.
      // Never misattribute that event to the old host's native lifecycle action.
      const closed = events.filter(e => e.pid === ready.hostPid && e.event === 'tabs' && e.closed.includes(ready.uri));
      cases.push({ action, ready, before, after: snapshot(), matchingClosedEvents: closed, actualClosed: closed.some(e => !e.open.includes(ready.uri)), cancelled: true, fresh: action === 'reload-window' ? json(`${base}/fresh.json`) : undefined });
      write(`${base}/result.json`, cases.at(-1));
    } finally {
      clearInterval(watcher);
      const survivors = census();
      write(`${base}/cleanup-before.json`, survivors);
      for (const p of survivors) { try { process.kill(p.pid, 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') throw e; } }
      await wait(() => census().length === 0, 'owned cleanup', undefined, 5000);
      write(`${base}/cleanup.json`, { remaining: census(), owned: [...owned.values()] });
    }
  }
  write(`${directory}/real-file-lifecycle-evidence.json`, { status: 'pass', cases });
} catch (error) {
  writeFileSync(`${directory}/failure.txt`, error.stack);
  console.error(error);
  process.exitCode = 1;
}
