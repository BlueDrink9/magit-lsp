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
if (native) {
  const child = spawn(editor, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  child.stdout.on('data', data => { stdout += data; });
  child.stderr.on('data', data => { stderr += data; });
  await new Promise(resolve => {
    const deadline = Date.now() + 90000;
    const timer = setInterval(() => {
      if (existsSync(`${dir}/result.json`) || child.exitCode !== null || Date.now() > deadline) { clearInterval(timer); resolve(); }
    }, 25);
  });
  result = { stdout, stderr, status: existsSync(`${dir}/result.json`) ? 0 : 1 };
} else {
  result = spawnSync(editor, args, { env, encoding: 'utf8', timeout: 90000 });
}
writeFileSync(`${dir}/output.txt`, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
// Match only our unique profile, never another editor instance.
try { for (const pid of execFileSync('pgrep', ['-f', `${dir}/user`], { encoding: 'utf8' }).trim().split('\n').filter(Boolean)) {
  try { process.kill(Number(pid), 'SIGTERM'); } catch (_) { /* already exited */ }
} } catch (_) { /* none alive */ }
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
