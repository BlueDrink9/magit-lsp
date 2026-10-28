import { existsSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, copyFileSync } from 'node:fs';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { createServer } from 'node:net';
const root = resolve(import.meta.dirname, '../../../..');
if (process.argv[2] === '--cdp') {
  const [, , , port, method, params] = process.argv;
  const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = pages.find(p => p.type === 'page' && p.url.includes('workbench'));
  if (!page) throw new Error('No isolated workbench');
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  const response = new Promise((resolve, reject) => { socket.onmessage = ({data}) => {
    const m = JSON.parse(data); if (m.id === 1) m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
  }; });
  socket.send(JSON.stringify({ id: 1, method, params: JSON.parse(params) }));
  console.log(JSON.stringify(await response)); socket.close(); process.exit(0);
}
if (process.argv.includes('--lifecycle')) {
  await lifecycle();
  process.exit(0);
}

/** Supervise isolated normal hosts across native exit/restart, retaining durable observations. */
async function lifecycle() {
  const editor = process.env.MAGIT_VSCODE_BIN || '/home/william/.nix-profile/bin/codium';
  const readLines = path => existsSync(path) ? readFileSync(path, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
  const wait = (check, label, timeout = 90000) => new Promise((resolve, reject) => {
    const deadline = Date.now() + timeout;
    const timer = setInterval(() => {
      try {
        const value = check();
        if (value) { clearInterval(timer); resolve(value); }
        else if (Date.now() > deadline) { clearInterval(timer); reject(new Error(`Deadline: ${label}`)); }
      } catch (error) { clearInterval(timer); reject(error); }
    }, 25);
  });
  for (const action of ['close-window', 'reload-window', 'workspace-change']) {
    const dir = mkdtempSync('/tmp/opencode/real-file-lifecycle-');
    const record = data => writeFileSync(`${dir}/supervisor.jsonl`, JSON.stringify({ timestamp: new Date().toISOString(), ...data }) + '\n', { flag: 'a' });
    execFileSync('git', ['init', '-q', `${dir}/repo`]);
    writeFileSync(`${dir}/repo/.git/COMMIT_EDITMSG`, 'spike message\n');
    mkdirSync(`${dir}/other-workspace`);
    mkdirSync(`${dir}/extension`);
    writeFileSync(`${dir}/extension/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));
    copyFileSync(`${root}/clients/vscode/test/e2e/real-file-spike.js`, `${dir}/extension/extension.js`);
    mkdirSync(`${dir}/user/User`, { recursive: true });
    writeFileSync(`${dir}/user/User/settings.json`, JSON.stringify({ 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'window.title': dir, 'git.enabled': false }));
    const server = createServer();
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const port = server.address().port;
    await new Promise(resolve => server.close(resolve));
    const env = { ...process.env, MAGIT_REAL_FILE_ROOT: root, MAGIT_REAL_FILE_EVIDENCE: dir,
      MAGIT_REAL_FILE_CDP: String(port), MAGIT_REAL_FILE_LIFECYCLE: action };
    delete env.ELECTRON_RUN_AS_NODE;
    const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust',
      `--remote-debugging-port=${port}`, '--user-data-dir', `${dir}/user`, '--extensions-dir', `${dir}/extensions`,
      `--extensionDevelopmentPath=${dir}/extension`, `${dir}/repo`];
    writeFileSync(`${dir}/command.json`, JSON.stringify({ editor, args, versions: execFileSync(editor, ['--version'], { encoding: 'utf8' }) }, null, 2));
    console.log(`Lifecycle ${action}: ${dir}`);
    let child, watcher;
    try {
      child = spawn(editor, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
      child.stdout.on('data', data => writeFileSync(`${dir}/stdout.txt`, data, { flag: 'a' }));
      child.stderr.on('data', data => writeFileSync(`${dir}/stderr.txt`, data, { flag: 'a' }));
      let launchError;
      child.on('error', error => { launchError = error; });
      child.on('exit', (code, signal) => record({ event: 'launcher-exit', code, signal }));
      record({ event: 'spawn', pid: child.pid, action });
      let wireCount = 0;
      watcher = setInterval(() => {
        const messages = readLines(`${dir}/wire.jsonl`);
        for (; wireCount < messages.length; wireCount++) record({ event: 'wire-observed', index: wireCount, message: messages[wireCount] });
      }, 25);
      await wait(() => {
        if (launchError) throw launchError;
        if (existsSync(`${dir}/lifecycle-error.json`)) throw new Error(readFileSync(`${dir}/lifecycle-error.json`, 'utf8'));
        return existsSync(`${dir}/lifecycle-action.json`);
      }, 'native action issued');
      await wait(() => action === 'close-window' ? child.exitCode !== null : existsSync(`${dir}/reactivated.json`), 'native lifecycle completion');
      if (action === 'close-window') {
        await wait(() => spawnSync('pgrep', ['-f', `${dir}/user`]).status === 1, 'native window processes exited', 10000);
        if (child.exitCode !== 0) throw new Error(`Native close exited ${child.exitCode}`);
      }
      const events = readLines(`${dir}/api.jsonl`);
      if (!events.some(e => e.event === 'deactivate')) throw new Error('Missing actual deactivation');
      if (action !== 'close-window') {
        const resumed = events.find(e => e.event === 'lifecycle-reactivated');
        const workspace = `file://${dir}/${action === 'workspace-change' ? 'other-workspace' : 'repo'}`;
        if (!resumed?.workspace?.includes(workspace)) throw new Error('Unexpected reactivated workspace');
      }
      const qualified = events.filter(e => e.event === 'tabs' && e.qualified);
      const result = { gate: 'pass', action, outcome: action === 'close-window' ? 'launcher-exited' : 'extension-reactivated',
        qualifyingClosures: qualified.length, blocker: qualified.length > 0,
        activations: events.filter(e => e.event === 'activate').length,
        deactivations: events.filter(e => e.event === 'deactivate').length,
        wireMethods: readLines(`${dir}/wire.jsonl`).map(m => m.method).filter(Boolean),
        confirmation: 'none: observer only; EOF/deactivation never confirms' };
      writeFileSync(`${dir}/result.json`, JSON.stringify(result, null, 2));
      record({ event: 'observation-complete', result });
      console.log(JSON.stringify(result));
      if (result.blocker) { console.log('BLOCKER: lifecycle emitted qualifying closure; stop before production design.'); return; }
    } finally {
      if (watcher) clearInterval(watcher);
      try {
        for (const pid of execFileSync('pgrep', ['-f', `${dir}/user`], { encoding: 'utf8' }).trim().split('\n').filter(Boolean)) {
          try { process.kill(Number(pid), 'SIGKILL'); } catch (_) { /* exited */ }
        }
      } catch (_) { /* no owned processes */ }
      await wait(() => spawnSync('pgrep', ['-f', `${dir}/user`]).status === 1, 'owned processes absent', 5000);
      record({ event: 'cleanup-complete' });
    }
  }
}
const editor = process.env.MAGIT_VSCODE_BIN || '/home/william/.nix-profile/bin/codium';
const dir = mkdtempSync('/tmp/opencode/real-file-vscode-');
const repo = `${dir}/repo`;
execFileSync('git', ['init', '-q', repo]);
writeFileSync(`${repo}/.git/COMMIT_EDITMSG`, 'spike message\n');
const ext = `${dir}/extension`; mkdirSync(ext);
writeFileSync(`${ext}/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));
copyFileSync(`${root}/clients/vscode/test/e2e/real-file-spike.js`, `${ext}/extension.js`);
mkdirSync(`${dir}/user/User`, { recursive: true });
writeFileSync(`${dir}/user/User/settings.json`, JSON.stringify({ 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'window.title': dir, 'git.enabled': false }));
const listener = createServer();
await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
const port = listener.address().port;
await new Promise(resolve => listener.close(resolve));
const env = { ...process.env, MAGIT_REAL_FILE_ROOT: root, MAGIT_REAL_FILE_EVIDENCE: dir, MAGIT_REAL_FILE_CDP: String(port) };
const native = process.argv.includes('--native');
if (native) env.MAGIT_REAL_FILE_NATIVE = '1';
delete env.ELECTRON_RUN_AS_NODE;
const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust',
  `--remote-debugging-port=${port}`, '--user-data-dir', `${dir}/user`, '--extensions-dir', `${dir}/extensions`,
  `--extensionDevelopmentPath=${ext}`, ...(!native ? [`--extensionTestsPath=${root}/clients/vscode/test/e2e/real-file-spike.js`] : []), repo];
writeFileSync(`${dir}/command.json`, JSON.stringify({ editor, args, versions: execFileSync(editor, ['--version'], { encoding: 'utf8' }) }, null, 2));
let result;
try {
if (native) {
  const child = spawn(editor, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  let launchError;
  child.on('error', error => { launchError = error; });
  child.stdout.on('data', data => { stdout += data; });
  child.stderr.on('data', data => { stderr += data; });
  await new Promise(resolve => {
    const deadline = Date.now() + 90000;
    const timer = setInterval(() => {
      if (existsSync(`${dir}/result.json`) || launchError || child.exitCode !== null || Date.now() > deadline) { clearInterval(timer); resolve(); }
    }, 25);
  });
  result = { stdout, stderr, error: launchError, status: existsSync(`${dir}/result.json`) && !launchError ? 0 : 1 };
} else {
  result = spawnSync(editor, args, { env, encoding: 'utf8', timeout: 90000 });
}
writeFileSync(`${dir}/output.txt`, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
} finally {
// Abrupt cleanup is intentional: SIGTERM can leave a dirty native dialog open.
// This is not evidence of shutdown semantics. Match only our unique profile.
try { for (const pid of execFileSync('pgrep', ['-f', `${dir}/user`], { encoding: 'utf8' }).trim().split('\n').filter(Boolean)) {
  try { process.kill(Number(pid), 'SIGKILL'); } catch (_) { /* already exited */ }
} } catch (_) { /* none alive */ }
await new Promise((resolve, reject) => {
  const deadline = Date.now() + 5000;
  const timer = setInterval(() => {
    const owned = spawnSync('pgrep', ['-f', `${dir}/user`], { encoding: 'utf8' });
    if (owned.status === 1) { clearInterval(timer); resolve(); }
    else if (Date.now() > deadline) { clearInterval(timer); reject(new Error(`Owned profile survived cleanup: ${dir}`)); }
  }, 25);
});
}
console.log(`Evidence: ${dir}`);
if (existsSync(`${dir}/result.json`)) console.log(readFileSync(`${dir}/result.json`, 'utf8'));
else console.error('INCOMPLETE: extension host produced no result', result.error || result.stderr);
let nativeStatus = 0;
if (!native) {
  const normal = spawnSync(process.execPath, [import.meta.filename, '--native'], { env: process.env, encoding: 'utf8', timeout: 105000 });
  console.log('Normal host dirty-dialog investigation:\n' + normal.stdout);
  if (normal.stderr) console.error(normal.stderr);
  nativeStatus = normal.status;
}
process.exit(nativeStatus === 0 && result.status === 0 && existsSync(`${dir}/result.json`) && JSON.parse(readFileSync(`${dir}/result.json`)).gate === 'pass' && !JSON.parse(readFileSync(`${dir}/result.json`)).error ? 0 : 1);
