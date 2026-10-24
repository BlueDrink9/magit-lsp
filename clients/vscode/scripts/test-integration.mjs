import { existsSync, mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

// Launch an isolated real editor; never reuse or close the user's instance.
const root = resolve(import.meta.dirname, '../../..');
const directory = mkdtempSync('/tmp/opencode/vscode-review-');
const repo = `${directory}/repo`;
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
const result = spawnSync(process.env.MAGIT_VSCODE_BIN || '/usr/bin/codium', [
  '--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes',
  '--disable-workspace-trust', '--user-data-dir', `${directory}/user`,
  '--extensions-dir', `${directory}/extensions`,
  `--extensionDevelopmentPath=${root}/clients/vscode`,
  `--extensionTestsPath=${root}/clients/vscode/test/integration.js`, repo,
], { env, stdio: 'inherit', timeout: 90000 });
console.log(`VSCode evidence: ${directory}`);
if (result.error) console.error(result.error);
if (existsSync(`${directory}/failure.txt`)) {
  console.error(await (await import('node:fs/promises')).readFile(`${directory}/failure.txt`, 'utf8'));
}
// Some editor CLIs return zero even if the test host failed to start.
process.exit(result.status === 0 && existsSync(`${directory}/evidence.json`) ? 0 : 1);
