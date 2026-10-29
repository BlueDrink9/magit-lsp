import { existsSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { execFileSync, spawnSync, spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';
import { createServer } from 'node:net';

/**
 * Optional real-editor suite: launch a unique Extension Development Host and
 * run `suite.js` inside it. The suite is skipped, not failed, when no editor
 * binary or display is available, so `make vscode-test` stays portable.
 */
const root = resolve(import.meta.dirname, '../../../..');
const candidates = [process.env.MAGIT_VSCODE_BIN, '/usr/bin/codium', '/usr/bin/code']
  .filter(Boolean);
const editor = candidates.find((candidate) => existsSync(candidate));
const focusedEdit = process.env.MAGIT_VSCODE_EDIT_ONLY === '1';
const focusedHead = process.env.MAGIT_VSCODE_HEAD_ONLY === '1';
if (!editor) {
  console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: no \`code\`/\`codium\` binary found (set MAGIT_VSCODE_BIN to run it)`);
  process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
}
if (!process.env.DISPLAY) {
  console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: DISPLAY is required for the real-editor keyboard checks`);
  process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
}
if (!existsSync(`${root}/target/debug/magit-lsp`)) {
  console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: missing target/debug/magit-lsp; run cargo build first`);
  process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
}

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
  const output = JSON.stringify(await response);
  await new Promise((resolve, reject) => process.stdout.write(`${output}\n`,
    (error) => error ? reject(error) : resolve()));
  socket.close();
  process.exit(0);
}

// Launch an isolated real editor; never reuse or close the user's instance.
const directory = mkdtempSync('/tmp/opencode/vscode-review-');
const repo = `${directory}/repo`;
const mirror = createRequire(import.meta.url)('../../lib/foldkeys.js').mapFoldKeys([
  { key: 'ctrl+alt+]', command: 'editor.toggleFold' },
]);
mkdirSync(`${directory}/user/User`, { recursive: true });
// XTEST sends keysyms, while this desktop may use Colemak (KeyT maps to G in
// VSCode's scan-code dispatcher). Resolve the actual keyCode in this isolated
// profile, without changing the user's XKB layout or any Magit context/binding.
writeFileSync(`${directory}/user/User/settings.json`, JSON.stringify({
  'keyboard.dispatch': 'keyCode',
  'window.dialogStyle': 'custom',
}));
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
  ...(process.env.MAGIT_VSCODE_A33_ONLY === '1' ? { 'magit-lsp.profile': 'vim' } : {}),
  'security.workspace.trust.enabled': false,
  'workbench.colorTheme': 'Default Dark Modern',
  'window.zoomLevel': 0,
  'window.title': directory,
}));
// Keep editor subprocesses free of giant inherited shell/function variables.
const env = Object.fromEntries(['HOME', 'PATH', 'DISPLAY', 'XAUTHORITY',
  'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'LANG', 'USER', 'SHELL']
  .filter((key) => process.env[key]).map((key) => [key, process.env[key]]));
if (process.env.MAGIT_VSCODE_A06_ONLY === '1') env.MAGIT_VSCODE_A06_ONLY = '1';
if (process.env.MAGIT_VSCODE_A29_ONLY === '1') env.MAGIT_VSCODE_A29_ONLY = '1';
if (process.env.MAGIT_VSCODE_A33_ONLY === '1') env.MAGIT_VSCODE_A33_ONLY = '1';
if (process.env.MAGIT_VSCODE_A34_ONLY === '1') env.MAGIT_VSCODE_A34_ONLY = '1';
if (focusedEdit) env.MAGIT_VSCODE_EDIT_ONLY = '1';
env.MAGIT_VSCODE_EVIDENCE = directory;
const listener = createServer();
await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
env.MAGIT_VSCODE_CDP_PORT = String(listener.address().port);
await new Promise((resolve) => listener.close(resolve));

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
      process.kill(pid, 'SIGKILL');
    } catch {
      // Already gone.
    }
  }
}

// Extension test hosts suppress native dirty-close dialogs. A tiny normal-host
// launcher runs the same production suite without extensionTestsPath instead.
if (focusedEdit || focusedHead) {
  mkdirSync(`${directory}/launcher`);
  writeFileSync(`${directory}/launcher/package.json`, JSON.stringify({
    publisher: 'test', name: 'native-edit-launcher', version: '1.0.0',
    engines: { vscode: '^1.85.0' }, main: './launcher.js', activationEvents: ['*'],
  }));
  writeFileSync(`${directory}/launcher/launcher.js`, `
    /** Run production acceptance in a normal extension host. */
    exports.activate = function () {
      setImmediate(async () => {
        let status = 0;
        try { await require(${JSON.stringify(`${root}/clients/vscode/test/e2e/suite.js`)}).run(); }
        catch (error) { status = 1; console.error(error); }
        require('node:fs').writeFileSync(${JSON.stringify(`${directory}/native-complete.json`)}, JSON.stringify({ status }));
      });
    };
  `);
}
if (focusedHead) env.MAGIT_VSCODE_HEAD_ONLY = '1';
const args = [
  '--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes',
  `--remote-debugging-port=${env.MAGIT_VSCODE_CDP_PORT}`,
  '--disable-workspace-trust', '--user-data-dir', `${directory}/user`,
  '--extensions-dir', `${directory}/extensions`,
  `--extensionDevelopmentPath=${root}/clients/vscode`,
  ...(focusedEdit || focusedHead ? [`--extensionDevelopmentPath=${directory}/launcher`]
    : [`--extensionTestsPath=${root}/clients/vscode/test/e2e/suite.js`]), repo,
];
let result;
if (focusedEdit || focusedHead) {
  const child = spawn(editor, args, { env, stdio: 'inherit' });
  let launchError;
  child.on('error', (error) => { launchError = error; });
  const deadline = Date.now() + 180000;
  await new Promise((resolve) => {
    const timer = setInterval(() => {
      if (launchError || child.exitCode !== null || existsSync(`${directory}/native-complete.json`)
        || Date.now() > deadline) { clearInterval(timer); resolve(); }
    }, 30);
  });
  result = { status: existsSync(`${directory}/native-complete.json`)
    ? JSON.parse((await import('node:fs')).readFileSync(`${directory}/native-complete.json`)).status : 1,
  error: launchError };
} else {
  result = spawnSync(editor, args, { env, stdio: 'inherit', timeout: 180000 });
}
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
// Some editor CLIs return zero even if the test host failed to start. The
// R-VS-2, R-VS-3, R-VS-4 and R-VS-5 gates must all have written their evidence.
const headEvidence = focusedHead && existsSync(`${directory}/head-operations-evidence.json`)
  ? JSON.parse(readFileSync(`${directory}/head-operations-evidence.json`, 'utf8')) : null;
const evidenceReady = focusedHead ? headEvidence?.status === 'pass' && headEvidence.cases?.length === 8 : focusedEdit
  ? existsSync(`${directory}/real-file-native-evidence.json`)
  : process.env.MAGIT_VSCODE_A34_ONLY === '1'
  ? existsSync(`${directory}/a34-evidence.json`)
  : process.env.MAGIT_VSCODE_A33_ONLY === '1'
  ? existsSync(`${directory}/a33-evidence.json`)
  : process.env.MAGIT_VSCODE_A29_ONLY === '1'
  ? existsSync(`${directory}/a29-evidence.json`)
  : process.env.MAGIT_VSCODE_A06_ONLY === '1'
  ? existsSync(`${directory}/a06-evidence.json`)
  : existsSync(`${directory}/r-vs-2-evidence.json`)
    && existsSync(`${directory}/r-vs-3-evidence.json`)
    && existsSync(`${directory}/r-vs-4-evidence.json`)
    && existsSync(`${directory}/r-vs-5-evidence.json`)
    && existsSync(`${directory}/a06-evidence.json`);
if (!evidenceReady) {
  console.error(
    focusedHead
      ? 'Missing complete native HEAD operations evidence; see the isolated evidence directory above.'
      : focusedEdit
      ? 'Missing production real-file native evidence; see the isolated evidence directory above.'
      : process.env.MAGIT_VSCODE_A34_ONLY === '1'
      ? 'Missing A34 evidence; see the isolated evidence directory above.'
      : process.env.MAGIT_VSCODE_A33_ONLY === '1'
      ? 'Missing A33 evidence; see the isolated evidence directory above.'
      : process.env.MAGIT_VSCODE_A29_ONLY === '1'
      ? 'Missing A29 evidence; see the isolated evidence directory above.'
      : process.env.MAGIT_VSCODE_A06_ONLY === '1'
      ? 'Missing A06 evidence; see the isolated evidence directory above.'
      : 'Missing R-VS-2/R-VS-3/R-VS-4/R-VS-5/A06 evidence; see the isolated evidence directory above.'
  );
}
process.exit(result.status === 0 && evidenceReady ? 0 : 1);
