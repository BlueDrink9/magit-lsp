import { existsSync, mkdtempSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';
import { createServer } from 'node:net';

// This subprocess observes the isolated renderer; it never sets context keys or
// dispatches commands. Page.captureScreenshot captures Monaco's actual surface.
if (process.argv[2] === '--cdp') {
  const [, , , port, method, params] = process.argv;
  const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = pages.find((entry) => entry.type === 'page' && entry.url.includes('workbench'));
  if (!page) throw new Error('Isolated workbench CDP target not found');
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  const response = new Promise((resolve, reject) => {
    socket.onmessage = ({ data }) => {
      const message = JSON.parse(data);
      if (message.id === 1) message.error ? reject(new Error(JSON.stringify(message.error))) : resolve(message.result);
    };
  });
  socket.send(JSON.stringify({ id: 1, method, params: JSON.parse(params) }));
  console.log(JSON.stringify(await response));
  socket.close();
  process.exit(0);
}

// Launch an isolated real editor; never reuse or close the user's instance.
const root = resolve(import.meta.dirname, '../../..');
const directory = mkdtempSync('/tmp/opencode/vscode-review-');
const repo = `${directory}/repo`;
const mirror = createRequire(import.meta.url)('../lib/foldkeys.js').mapFoldKeys([
  { key: 'ctrl+alt+]', command: 'editor.toggleFold' },
]);
mkdirSync(`${directory}/user/User`, { recursive: true });
writeFileSync(`${directory}/user/User/keybindings.json`, JSON.stringify(mirror, null, 2));
writeFileSync(`${directory}/mirrored-bindings.json`, JSON.stringify(mirror, null, 2));
mkdirSync(`${repo}/.vscode`, { recursive: true });
execFileSync('git', ['init', '-q', repo]);
writeFileSync(`${repo}/tracked.txt`, 'original\n');
execFileSync('git', ['-C', repo, 'add', '.']);
execFileSync('git', ['-C', repo, '-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '-qm', 'initial']);
writeFileSync(`${repo}/tracked.txt`, 'replacement\n');
writeFileSync(`${repo}/.vscode/settings.json`, JSON.stringify({
  'magit-lsp.serverPath': `${root}/target/debug/magit-lsp`,
  'security.workspace.trust.enabled': false,
  'workbench.colorTheme': 'Default Dark Modern',
  'window.zoomLevel': 0,
  'window.title': directory,
}));
// Keep editor subprocesses free of giant inherited shell/function variables.
const env = Object.fromEntries(['HOME', 'PATH', 'DISPLAY', 'XAUTHORITY',
  'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'LANG', 'USER', 'SHELL']
  .filter((key) => process.env[key]).map((key) => [key, process.env[key]]));
env.MAGIT_VSCODE_EVIDENCE = directory;
const listener = createServer();
await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
env.MAGIT_VSCODE_CDP_PORT = String(listener.address().port);
await new Promise((resolve) => listener.close(resolve));
if (!existsSync(`${root}/target/debug/magit-lsp`)) {
  console.error('Missing target/debug/magit-lsp; run cargo build first.');
  process.exit(1);
}
if (!process.env.DISPLAY) {
  console.error('DISPLAY is required for the real-client keyboard checks.');
  process.exit(1);
}
const editor = process.env.MAGIT_VSCODE_BIN || '/usr/bin/codium';

/**
 * PIDs whose command line names this run's unique user-data directory.
 *
 * @param {string} base
 * @returns {number[]}
 */
function ownedPids(base) {
  try {
    return execFileSync('pgrep', ['-f', '--', `${base}/user`])
      .toString().trim().split('\n').filter(Boolean).map(Number);
  } catch {
    return [];
  }
}

/** Terminate only editor processes launched by this script. */
function stopOwned(base) {
  for (const pid of ownedPids(base)) {
    try {
      process.kill(pid, 'SIGTERM');
    } catch {
      // Already gone.
    }
  }
}

const result = spawnSync(editor, [
  '--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes',
  `--remote-debugging-port=${env.MAGIT_VSCODE_CDP_PORT}`,
  '--disable-workspace-trust', '--user-data-dir', `${directory}/user`,
  '--extensions-dir', `${directory}/extensions`,
  `--extensionDevelopmentPath=${root}/clients/vscode`,
  `--extensionTestsPath=${root}/clients/vscode/test/integration.js`, repo,
], { env, stdio: 'inherit', timeout: 180000 });
stopOwned(directory);
console.log(`VSCode evidence: ${directory}`);
try {
  console.log('Screenshots:', readdirSync(directory).filter((name) => name.endsWith('.png')).join(', '));
} catch {
  // Evidence directory survived; listing is informational only.
}
if (result.error) console.error(result.error);
if (existsSync(`${directory}/failure.txt`)) {
  console.error(await (await import('node:fs/promises')).readFile(`${directory}/failure.txt`, 'utf8'));
}
// Some editor CLIs return zero even if the test host failed to start.
process.exit(result.status === 0 && existsSync(`${directory}/r-vs-5-evidence.json`) ? 0 : 1);
