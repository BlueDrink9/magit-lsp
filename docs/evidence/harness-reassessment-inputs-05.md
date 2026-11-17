### `clients/vscode/test/integration.js` lines 580-653 of 653
```text
 580 |       await idle(draft);
 581 |       assert.equal(head(), priorHead, action);
 582 |       assert.deepEqual(unchangedIndex(), priorIndex, action);
 583 |       if (draftStore.drafts.has(draft.runId)) await commands.get('magit.cancelEdit')();
 584 |     }
 585 |     const synced = client.notes.filter((note) => note.method === 'textDocument/didChange');
 586 |     assert(synced.every((note) => note.params.contentChanges.length === 1
 587 |       && Object.keys(note.params.contentChanges[0]).join() === 'text'));
 588 |
 589 |     // The displayed text is always the server's last render; never recomposed.
 590 |     assert.match(statusEditor.document.getText(), /Unstaged changes/);
 591 |     assert.equal(vscode.__test.output.filter((line) => line.startsWith('unknown effect')).length, 0);
 592 |
 593 |     // Quit is dispatched through the registered command and scoped to the
 594 |     // repository that owns the originating view. This harness uses mocked
 595 |     // VSCode APIs; it is not evidence of native tab behavior.
 596 |     const { store } = require('../lib/store');
 597 |     const secondRepo = createRepository();
 598 |     vscode.__test.setWorkspaceFolder(secondRepo);
 599 |     await commands.get('magit.open')();
 600 |     await waitFor(() => store.views.size === 2, 'second repository view');
 601 |     await commands.get('magit.invoke')({ command: 'common.quit' });
 602 |     assert.equal(store.views.size, 1, 'quit removes only the originating repository view');
 603 |     assert.equal(Array.from(store.views.values())[0].repo, repo);
 604 |     vscode.__test.setWorkspaceFolder(repo);
 605 |     await commands.get('magit.open')();
 606 |     await commands.get('magit.invoke')({ command: 'common.quit' });
 607 |     assert.equal(store.views.size, 0, 'root quit removes the affected stored view');
 608 |     await commands.get('magit.open')();
 609 |     await waitFor(() => store.views.size === 1, 'reopened repository view');
 610 |     fs.rmSync(secondRepo, { recursive: true, force: true });
 611 |     await require('./selection-integration').checkGitSelections({ vscode, commands, client });
 612 |     // A disconnected owner cannot finish or restart against a new connection;
 613 |     // server shutdown aborts the pending helper, while native text is retained.
 614 |     vscode.__test.setWorkspaceFolder(repo);
 615 |     await commands.get('magit.open')();
 616 |     fs.writeFileSync(path.join(repo, 'tracked.txt'), 'disconnect draft probe\n');
 617 |     git(repo, ['add', 'tracked.txt']);
 618 |     await commands.get('magit.invoke')({ command: 'commit.create' });
 619 |     await waitFor(() => [...draftStore.drafts.values()].some((record) => record.attached), 'disconnect draft');
 620 |     const disconnected = [...draftStore.drafts.values()].find((record) => record.attached);
 621 |     disconnected.document.replaceText('retain disconnected text\n');
 622 |     const disconnectHead = head();
 623 |     await extension.deactivate();
 624 |     assert.equal(head(), disconnectHead, 'disconnect cancels the pending commit');
 625 |     assert.equal(disconnected.document.getText(), 'retain disconnected text\n');
 626 |     assert(disconnected.disconnected);
 627 |   } finally {
 628 |     await extension.deactivate();
 629 |   }
 630 |
 631 |   const exited = await Promise.race([
 632 |     instances.at(-1).exited.then(() => true),
 633 |     delay(5000).then(() => false),
 634 |   ]);
 635 |   assert(exited, 'the language server exited after deactivate');
 636 |   fs.rmSync(repo, { recursive: true, force: true });
 637 | }
 638 |
 639 | run().then(() => {
 640 |   console.log('ok node integration');
 641 | }).catch((error) => {
 642 |   console.error(error && error.stack ? error.stack : error);
 643 |   for (const client of instances) {
 644 |     if (client.stderr) console.error(`--- magit-lsp stderr ---\n${client.stderr}`);
 645 |   }
 646 |   for (const message of vscode.__test.messages) {
 647 |     console.error(`--- ${message.level}: ${message.text}`);
 648 |   }
 649 |   for (const line of vscode.__test.output) {
 650 |     console.error(`--- output: ${line}`);
 651 |   }
 652 |   process.exitCode = 1;
 653 | });
```

### Native launch, environment, CDP, key/dialog handling

### `clients/vscode/test/e2e/run.mjs` lines 1-36 of 218
```text
   1 | import { existsSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
   2 | import { execFileSync, spawnSync, spawn } from 'node:child_process';
   3 | import { resolve } from 'node:path';
   4 | import { createRequire } from 'node:module';
   5 | import { createServer } from 'node:net';
   6 |
   7 | /**
   8 |  * Optional real-editor suite: launch a unique Extension Development Host and
   9 |  * run `suite.js` inside it. The suite is skipped, not failed, when no editor
  10 |  * binary or display is available, so `make vscode-test` stays portable.
  11 |  */
  12 | const root = resolve(import.meta.dirname, '../../../..');
  13 | const candidates = [process.env.MAGIT_VSCODE_BIN, '/usr/bin/codium', '/usr/bin/code']
  14 |   .filter(Boolean);
  15 | const editor = candidates.find((candidate) => existsSync(candidate));
  16 | const focusedEdit = process.env.MAGIT_VSCODE_EDIT_ONLY === '1';
  17 | const focusedHead = process.env.MAGIT_VSCODE_HEAD_ONLY === '1';
  18 | if (!editor) {
  19 |   console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: no \`code\`/\`codium\` binary found (set MAGIT_VSCODE_BIN to run it)`);
  20 |   process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
  21 | }
  22 | if (!process.env.DISPLAY) {
  23 |   console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: DISPLAY is required for the real-editor keyboard checks`);
  24 |   process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
  25 | }
  26 | if (!existsSync(`${root}/target/debug/magit-lsp`)) {
  27 |   console.log(`${focusedHead ? 'FAIL' : 'skip'} e2e: missing target/debug/magit-lsp; run cargo build first`);
  28 |   process.exit(focusedEdit || focusedHead || ['A29', 'A33', 'A34'].some((id) => process.env[`MAGIT_VSCODE_${id}_ONLY`] === '1') ? 1 : 0);
  29 | }
  30 |
  31 | // This subprocess observes the isolated renderer; it never sets context keys or
  32 | // dispatches commands. Image capture methods are explicitly forbidden.
  33 | if (process.argv[2] === '--cdp') {
  34 |   const [, , , port, method, params] = process.argv;
  35 |   if (/image|screenshot|snapshot/i.test(method)) {
  36 |     throw new Error(`Image-related CDP method is forbidden: ${method}`);
```
### `clients/vscode/test/e2e/run.mjs` lines 60-100 of 218
```text
  60 | const mirror = createRequire(import.meta.url)('../../lib/foldkeys.js').mapFoldKeys([
  61 |   { key: 'ctrl+alt+]', command: 'editor.toggleFold' },
  62 | ]);
  63 | mkdirSync(`${directory}/user/User`, { recursive: true });
  64 | // XTEST sends keysyms, while this desktop may use Colemak (KeyT maps to G in
  65 | // VSCode's scan-code dispatcher). Resolve the actual keyCode in this isolated
  66 | // profile, without changing the user's XKB layout or any Magit context/binding.
  67 | writeFileSync(`${directory}/user/User/settings.json`, JSON.stringify({
  68 |   'keyboard.dispatch': 'keyCode',
  69 |   'window.dialogStyle': 'custom',
  70 | }));
  71 | writeFileSync(`${directory}/user/User/keybindings.json`, JSON.stringify(mirror, null, 2));
  72 | writeFileSync(`${directory}/mirrored-bindings.json`, JSON.stringify(mirror, null, 2));
  73 | mkdirSync(`${repo}/.vscode`, { recursive: true });
  74 | execFileSync('git', ['init', '-q', repo]);
  75 | writeFileSync(`${repo}/tracked.txt`, 'original\n');
  76 | execFileSync('git', ['-C', repo, 'add', '.']);
  77 | execFileSync('git', ['-C', repo, '-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '-qm', 'initial']);
  78 | writeFileSync(`${repo}/tracked.txt`, 'replacement\n');
  79 | writeFileSync(`${repo}/.vscode/settings.json`, JSON.stringify({
  80 |   'magit-lsp.serverPath': `${root}/target/debug/magit-lsp`,
  81 |   ...(process.env.MAGIT_VSCODE_A33_ONLY === '1' ? { 'magit-lsp.profile': 'vim' } : {}),
  82 |   'security.workspace.trust.enabled': false,
  83 |   'workbench.colorTheme': 'Default Dark Modern',
  84 |   'window.zoomLevel': 0,
  85 |   'window.title': directory,
  86 | }));
  87 | // Keep editor subprocesses free of giant inherited shell/function variables.
  88 | const env = Object.fromEntries(['HOME', 'PATH', 'DISPLAY', 'XAUTHORITY',
  89 |   'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'LANG', 'USER', 'SHELL']
  90 |   .filter((key) => process.env[key]).map((key) => [key, process.env[key]]));
  91 | if (process.env.MAGIT_VSCODE_A06_ONLY === '1') env.MAGIT_VSCODE_A06_ONLY = '1';
  92 | if (process.env.MAGIT_VSCODE_A29_ONLY === '1') env.MAGIT_VSCODE_A29_ONLY = '1';
  93 | if (process.env.MAGIT_VSCODE_A33_ONLY === '1') env.MAGIT_VSCODE_A33_ONLY = '1';
  94 | if (process.env.MAGIT_VSCODE_A34_ONLY === '1') env.MAGIT_VSCODE_A34_ONLY = '1';
  95 | if (focusedEdit) env.MAGIT_VSCODE_EDIT_ONLY = '1';
  96 | env.MAGIT_VSCODE_EVIDENCE = directory;
  97 | const listener = createServer();
  98 | await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  99 | env.MAGIT_VSCODE_CDP_PORT = String(listener.address().port);
 100 | await new Promise((resolve) => listener.close(resolve));
```
### `clients/vscode/test/e2e/run.mjs` lines 108-180 of 218
```text
 108 | function ownedPids(base) {
 109 |   try {
 110 |     return execFileSync('pgrep', ['-f', '--', `${base}/user`])
 111 |       .toString().trim().split('\n').filter(Boolean).map(Number);
 112 |   } catch {
 113 |     return [];
 114 |   }
 115 | }
 116 |
 117 | /** Terminate only editor processes launched by this script. */
 118 | function stopOwned(base) {
 119 |   for (const pid of ownedPids(base)) {
 120 |     try {
 121 |       process.kill(pid, 'SIGKILL');
 122 |     } catch {
 123 |       // Already gone.
 124 |     }
 125 |   }
 126 | }
 127 |
 128 | // Extension test hosts suppress native dirty-close dialogs. A tiny normal-host
 129 | // launcher runs the same production suite without extensionTestsPath instead.
 130 | if (focusedEdit || focusedHead) {
 131 |   mkdirSync(`${directory}/launcher`);
 132 |   writeFileSync(`${directory}/launcher/package.json`, JSON.stringify({
 133 |     publisher: 'test', name: 'native-edit-launcher', version: '1.0.0',
 134 |     engines: { vscode: '^1.85.0' }, main: './launcher.js', activationEvents: ['*'],
 135 |   }));
 136 |   writeFileSync(`${directory}/launcher/launcher.js`, `
 137 |     /** Run production acceptance in a normal extension host. */
 138 |     exports.activate = function () {
 139 |       setImmediate(async () => {
 140 |         let status = 0;
 141 |         try { await require(${JSON.stringify(`${root}/clients/vscode/test/e2e/suite.js`)}).run(); }
 142 |         catch (error) { status = 1; console.error(error); }
 143 |         require('node:fs').writeFileSync(${JSON.stringify(`${directory}/native-complete.json`)}, JSON.stringify({ status }));
 144 |       });
 145 |     };
 146 |   `);
 147 | }
 148 | if (focusedHead) env.MAGIT_VSCODE_HEAD_ONLY = '1';
 149 | const args = [
 150 |   '--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes',
 151 |   `--remote-debugging-port=${env.MAGIT_VSCODE_CDP_PORT}`,
 152 |   '--disable-workspace-trust', '--user-data-dir', `${directory}/user`,
 153 |   '--extensions-dir', `${directory}/extensions`,
 154 |   `--extensionDevelopmentPath=${root}/clients/vscode`,
 155 |   ...(focusedEdit || focusedHead ? [`--extensionDevelopmentPath=${directory}/launcher`]
 156 |     : [`--extensionTestsPath=${root}/clients/vscode/test/e2e/suite.js`]), repo,
 157 | ];
 158 | let result;
 159 | if (focusedEdit || focusedHead) {
 160 |   const child = spawn(editor, args, { env, stdio: 'inherit' });
 161 |   let launchError;
 162 |   child.on('error', (error) => { launchError = error; });
 163 |   const deadline = Date.now() + 180000;
 164 |   await new Promise((resolve) => {
 165 |     const timer = setInterval(() => {
 166 |       if (launchError || child.exitCode !== null || existsSync(`${directory}/native-complete.json`)
 167 |         || Date.now() > deadline) { clearInterval(timer); resolve(); }
 168 |     }, 30);
 169 |   });
 170 |   result = { status: existsSync(`${directory}/native-complete.json`)
 171 |     ? JSON.parse((await import('node:fs')).readFileSync(`${directory}/native-complete.json`)).status : 1,
 172 |   error: launchError };
 173 | } else {
 174 |   result = spawnSync(editor, args, { env, stdio: 'inherit', timeout: 180000 });
 175 | }
 176 | stopOwned(directory);
 177 | console.log(`VSCode text evidence: ${directory}`);
 178 | if (result.error) console.error(result.error);
 179 | if (existsSync(`${directory}/failure.txt`)) {
 180 |   console.error(await (await import('node:fs/promises')).readFile(`${directory}/failure.txt`, 'utf8'));
```
### `clients/vscode/test/e2e/run.mjs` lines 186-218 of 218
```text
 186 | const evidenceReady = focusedHead ? headEvidence?.status === 'pass' && headEvidence.cases?.length === 8 : focusedEdit
 187 |   ? existsSync(`${directory}/real-file-native-evidence.json`)
 188 |   : process.env.MAGIT_VSCODE_A34_ONLY === '1'
 189 |   ? existsSync(`${directory}/a34-evidence.json`)
 190 |   : process.env.MAGIT_VSCODE_A33_ONLY === '1'
 191 |   ? existsSync(`${directory}/a33-evidence.json`)
 192 |   : process.env.MAGIT_VSCODE_A29_ONLY === '1'
 193 |   ? existsSync(`${directory}/a29-evidence.json`)
 194 |   : process.env.MAGIT_VSCODE_A06_ONLY === '1'
 195 |   ? existsSync(`${directory}/a06-evidence.json`)
 196 |   : existsSync(`${directory}/r-vs-2-evidence.json`)
 197 |     && existsSync(`${directory}/r-vs-3-evidence.json`)
 198 |     && existsSync(`${directory}/r-vs-4-evidence.json`)
 199 |     && existsSync(`${directory}/r-vs-5-evidence.json`)
 200 |     && existsSync(`${directory}/a06-evidence.json`);
 201 | if (!evidenceReady) {
 202 |   console.error(
 203 |     focusedHead
 204 |       ? 'Missing complete native HEAD operations evidence; see the isolated evidence directory above.'
 205 |       : focusedEdit
 206 |       ? 'Missing production real-file native evidence; see the isolated evidence directory above.'
 207 |       : process.env.MAGIT_VSCODE_A34_ONLY === '1'
 208 |       ? 'Missing A34 evidence; see the isolated evidence directory above.'
 209 |       : process.env.MAGIT_VSCODE_A33_ONLY === '1'
 210 |       ? 'Missing A33 evidence; see the isolated evidence directory above.'
 211 |       : process.env.MAGIT_VSCODE_A29_ONLY === '1'
 212 |       ? 'Missing A29 evidence; see the isolated evidence directory above.'
 213 |       : process.env.MAGIT_VSCODE_A06_ONLY === '1'
 214 |       ? 'Missing A06 evidence; see the isolated evidence directory above.'
 215 |       : 'Missing R-VS-2/R-VS-3/R-VS-4/R-VS-5/A06 evidence; see the isolated evidence directory above.'
 216 |   );
 217 | }
 218 | process.exit(result.status === 0 && evidenceReady ? 0 : 1);
```
### `clients/vscode/test/e2e/real-file-lifecycle.mjs` lines 1-20 of 130
```text
   1 | import assert from 'node:assert/strict';
   2 | import { existsSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, appendFileSync } from 'node:fs';
   3 | import { spawn, execFileSync } from 'node:child_process';
   4 | import { resolve } from 'node:path';
   5 | import { fileURLToPath } from 'node:url';
   6 | import { createServer } from 'node:net';
   7 |
   8 | const root = resolve(fileURLToPath(new URL('../../../..', import.meta.url)));
   9 | const editor = process.env.MAGIT_VSCODE_BIN;
  10 | assert(editor && existsSync(editor) && process.env.DISPLAY && existsSync(`${root}/target/debug/magit-lsp`), 'MAGIT_VSCODE_BIN, DISPLAY and built server required');
  11 | const directory = mkdtempSync('/tmp/opencode/real-file-lifecycle-');
  12 | console.log(`Evidence: ${directory}`);
  13 | const json = path => JSON.parse(readFileSync(path, 'utf8'));
  14 | const lines = path => existsSync(path) ? readFileSync(path, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
  15 | const write = (path, data) => writeFileSync(path, JSON.stringify(data, null, 2));
  16 | /** Poll observable state, surfacing durable host errors even after the host dies. */
  17 | async function wait(check, label, base, timeout = 90000) {
  18 |   const deadline = Date.now() + timeout;
  19 |   while (true) {
  20 |     if (base && existsSync(`${base}/failure.txt`)) throw Error(readFileSync(`${base}/failure.txt`, 'utf8'));
```
### `clients/vscode/test/e2e/real-file-lifecycle.mjs` lines 45-70 of 130
```text
  45 |     writeFileSync(`${repo}/tracked.txt`, `staged ${action}\n`); git('add', 'tracked.txt');
  46 |     writeFileSync(`${repo}/tracked.txt`, `unstaged ${action}\n`);
  47 |     const snapshot = () => ({ head: git('rev-parse', 'HEAD'), index: git('write-tree'), worktree: git('diff', '--binary'), status: git('status', '--porcelain') });
  48 |     const before = snapshot();
  49 |     mkdirSync(`${base}/other-workspace`); mkdirSync(`${base}/launcher`); mkdirSync(`${base}/user/User`, { recursive: true });
  50 |     write(`${base}/user/User/settings.json`, { 'magit-lsp.serverPath': `${root}/target/debug/magit-lsp`, 'window.title': base, 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'security.workspace.trust.enabled': false, 'git.enabled': false });
  51 |     const listener = createServer();
  52 |     await new Promise(r => listener.listen(0, '127.0.0.1', r));
  53 |     const port = listener.address().port;
  54 |     await new Promise(r => listener.close(r));
  55 |     write(`${base}/launcher/package.json`, { publisher: 'test', name: 'native-lifecycle', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './launcher.js', activationEvents: ['*'] });
  56 |     writeFileSync(`${base}/launcher/launcher.js`, `exports.activate = () => { setImmediate(() => require(${JSON.stringify(`${root}/clients/vscode/test/e2e/real-file-lifecycle.js`)}).run(${JSON.stringify({ root, evidence: base, repo, action, port })}).catch(e => require('fs').writeFileSync(${JSON.stringify(`${base}/failure.txt`)}, e.stack))); }; exports.deactivate = () => require('fs').appendFileSync(${JSON.stringify(`${base}/host.jsonl`)}, JSON.stringify({event:'deactivate',pid:process.pid,time:Date.now()})+'\\n');`);
  57 |     const env = Object.fromEntries(['HOME', 'PATH', 'DISPLAY', 'XAUTHORITY', 'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'LANG', 'USER', 'SHELL'].filter(k => process.env[k]).map(k => [k, process.env[k]]));
  58 |     const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust', `--remote-debugging-port=${port}`, '--user-data-dir', `${base}/user`, '--extensions-dir', `${base}/extensions`, `--extensionDevelopmentPath=${root}/clients/vscode`, `--extensionDevelopmentPath=${base}/launcher`, repo];
  59 |     write(`${base}/command.json`, { editor, args });
  60 |     const owned = new Map();
  61 |     const census = () => {
  62 |       const all = processes();
  63 |       let changed = true;
  64 |       while (changed) {
  65 |         changed = false;
  66 |         for (const p of all) if ((p.command.includes(`${base}/user`) || owned.has(p.parent)) && !owned.has(p.pid)) { owned.set(p.pid, p); changed = true; }
  67 |       }
  68 |       return all.filter(p => owned.get(p.pid)?.start === p.start && p.state !== 'Z');
  69 |     };
  70 |     let watcher;
```
### `clients/vscode/test/e2e/real-file-spike.mjs` lines 40-65 of 182
```text
  40 |     const record = data => writeFileSync(`${dir}/supervisor.jsonl`, JSON.stringify({ timestamp: new Date().toISOString(), ...data }) + '\n', { flag: 'a' });
  41 |     execFileSync('git', ['init', '-q', `${dir}/repo`]);
  42 |     writeFileSync(`${dir}/repo/.git/COMMIT_EDITMSG`, 'spike message\n');
  43 |     mkdirSync(`${dir}/other-workspace`);
  44 |     mkdirSync(`${dir}/extension`);
  45 |     writeFileSync(`${dir}/extension/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));
  46 |     copyFileSync(`${root}/clients/vscode/test/e2e/real-file-spike.js`, `${dir}/extension/extension.js`);
  47 |     mkdirSync(`${dir}/user/User`, { recursive: true });
  48 |     writeFileSync(`${dir}/user/User/settings.json`, JSON.stringify({ 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'window.title': dir, 'git.enabled': false }));
  49 |     const server = createServer();
  50 |     await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  51 |     const port = server.address().port;
  52 |     await new Promise(resolve => server.close(resolve));
  53 |     const env = { ...process.env, MAGIT_REAL_FILE_ROOT: root, MAGIT_REAL_FILE_EVIDENCE: dir,
  54 |       MAGIT_REAL_FILE_CDP: String(port), MAGIT_REAL_FILE_LIFECYCLE: action };
  55 |     delete env.ELECTRON_RUN_AS_NODE;
  56 |     const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust',
  57 |       `--remote-debugging-port=${port}`, '--user-data-dir', `${dir}/user`, '--extensions-dir', `${dir}/extensions`,
  58 |       `--extensionDevelopmentPath=${dir}/extension`, `${dir}/repo`];
  59 |     writeFileSync(`${dir}/command.json`, JSON.stringify({ editor, args, versions: execFileSync(editor, ['--version'], { encoding: 'utf8' }) }, null, 2));
  60 |     console.log(`Lifecycle ${action}: ${dir}`);
  61 |     let child, watcher;
  62 |     try {
  63 |       child = spawn(editor, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
  64 |       child.stdout.on('data', data => writeFileSync(`${dir}/stdout.txt`, data, { flag: 'a' }));
  65 |       child.stderr.on('data', data => writeFileSync(`${dir}/stderr.txt`, data, { flag: 'a' }));
```
### `clients/vscode/test/e2e/real-file-spike.mjs` lines 116-145 of 182
```text
 116 | const dir = mkdtempSync('/tmp/opencode/real-file-vscode-');
 117 | const repo = `${dir}/repo`;
 118 | execFileSync('git', ['init', '-q', repo]);
 119 | writeFileSync(`${repo}/.git/COMMIT_EDITMSG`, 'spike message\n');
 120 | const ext = `${dir}/extension`; mkdirSync(ext);
 121 | writeFileSync(`${ext}/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));
 122 | copyFileSync(`${root}/clients/vscode/test/e2e/real-file-spike.js`, `${ext}/extension.js`);
 123 | mkdirSync(`${dir}/user/User`, { recursive: true });
 124 | writeFileSync(`${dir}/user/User/settings.json`, JSON.stringify({ 'window.dialogStyle': 'custom', 'workbench.startupEditor': 'none', 'window.title': dir, 'git.enabled': false }));
 125 | const listener = createServer();
 126 | await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
 127 | const port = listener.address().port;
 128 | await new Promise(resolve => listener.close(resolve));
 129 | const env = { ...process.env, MAGIT_REAL_FILE_ROOT: root, MAGIT_REAL_FILE_EVIDENCE: dir, MAGIT_REAL_FILE_CDP: String(port) };
 130 | const native = process.argv.includes('--native');
 131 | if (native) env.MAGIT_REAL_FILE_NATIVE = '1';
 132 | delete env.ELECTRON_RUN_AS_NODE;
 133 | const args = ['--wait', '--no-sandbox', '--disable-gpu', '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust',
 134 |   `--remote-debugging-port=${port}`, '--user-data-dir', `${dir}/user`, '--extensions-dir', `${dir}/extensions`,
 135 |   `--extensionDevelopmentPath=${ext}`, ...(!native ? [`--extensionTestsPath=${root}/clients/vscode/test/e2e/real-file-spike.js`] : []), repo];
 136 | writeFileSync(`${dir}/command.json`, JSON.stringify({ editor, args, versions: execFileSync(editor, ['--version'], { encoding: 'utf8' }) }, null, 2));
 137 | let result;
 138 | try {
 139 | if (native) {
 140 |   const child = spawn(editor, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
 141 |   let stdout = '', stderr = '';
 142 |   let launchError;
 143 |   child.on('error', error => { launchError = error; });
 144 |   child.stdout.on('data', data => { stdout += data; });
 145 |   child.stderr.on('data', data => { stderr += data; });
```
### `clients/vscode/test/e2e/real-file-spike.js` lines 1-25 of 316
```text
   1 | 'use strict';
   2 | // Isolated test extension and extension-host suite; never loads production code.
   3 | const vscode = require('vscode');
   4 | const fs = require('fs');
   5 | const { execFileSync } = require('child_process');
   6 | const assert = require('node:assert/strict');
   7 | const base = process.env.MAGIT_REAL_FILE_EVIDENCE;
   8 | const root = process.env.MAGIT_REAL_FILE_ROOT;
   9 | const trace = () => fs.existsSync(`${base}/wire.jsonl`)
  10 |   ? fs.readFileSync(`${base}/wire.jsonl`, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
  11 | const count = (method) => trace().filter(m => m.method === method).length;
  12 |
  13 | /** Await a condition with a bounded deadline, not a fixed settling delay. */
  14 | function until(check, label, timeout = 5000) {
  15 |   return new Promise((resolve, reject) => {
  16 |     const deadline = Date.now() + timeout;
  17 |     const timer = setInterval(() => {
  18 |       try {
  19 |         const result = check();
  20 |         if (result) { clearInterval(timer); resolve(result); }
  21 |         else if (Date.now() >= deadline) { clearInterval(timer); reject(new Error(`Deadline: ${label}`)); }
  22 |       } catch (error) { clearInterval(timer); reject(error); }
  23 |     }, 25);
  24 |   });
  25 | }
```
### `clients/vscode/test/e2e/real-file-spike.js` lines 115-155 of 316
```text
 115 |     // still proved externally by exit/reactivation, never by this rejection.
 116 |     log({ event: 'lifecycle-command-rejected', command, error: error.stack });
 117 |   }
 118 | }
 119 |
 120 | /** Exercise the real editor, retaining observations even when the hard gate fails. */
 121 | exports.run = async function run() {
 122 |   const result = { gate: 'incomplete', cases: {} };
 123 |   let client;
 124 |   const cdp = (method, params = {}) => JSON.parse(execFileSync(process.execPath, [
 125 |     `${root}/clients/vscode/test/e2e/real-file-spike.mjs`, '--cdp', process.env.MAGIT_REAL_FILE_CDP, method, JSON.stringify(params),
 126 |   ], { env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' }, encoding: 'utf8', timeout: 10000 }));
 127 |   try {
 128 |     ({ client } = await vscode.extensions.getExtension('spike.real-file-spike').activate());
 129 |     const windowId = await until(() => {
 130 |       try {
 131 |         const ids = execFileSync('xdotool', ['search', '--name', base], { encoding: 'utf8' }).trim().split('\n');
 132 |         return ids.length === 1 && ids[0];
 133 |       } catch (_) { return false; }
 134 |     }, 'isolated window');
 135 |     try { execFileSync('bspc', ['node', windowId, '-t', 'floating'], { stdio: 'ignore' }); } catch (_) { /* other WM */ }
 136 |     execFileSync('xdotool', ['windowsize', '--sync', windowId, '1200', '760']);
 137 |     await until(() => cdp('Runtime.evaluate', { expression: 'innerWidth >= 1100' }).result.value, 'window geometry');
 138 |     const uri = vscode.Uri.file(`${base}/repo/.git/COMMIT_EDITMSG`);
 139 |     const api = () => fs.existsSync(`${base}/api.jsonl`)
 140 |       ? fs.readFileSync(`${base}/api.jsonl`, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
 141 |     const qualifiedClosures = () => api().filter(item => item.event === 'tabs' && item.qualified);
 142 |     const matchingTabs = () => vscode.window.tabGroups.all.flatMap(g => g.tabs)
 143 |       .filter(tab => tab.input?.uri?.toString() === uri.toString());
 144 |     const show = () => client.sendRequest('spike/show', { uri: uri.toString(), takeFocus: true });
 145 |     result.cases.show = await show();
 146 |     assert.equal(result.cases.show.success, true);
 147 |     assert.equal(api().filter(item => item.event === 'showDocument').length, 1, 'real middleware invoked');
 148 |     result.cases.preview = { isPreview: vscode.window.tabGroups.all.flatMap(g => g.tabs)
 149 |       .find(tab => tab.input && tab.input.uri && tab.input.uri.toString() === uri.toString()).isPreview };
 150 |     if (result.cases.preview.isPreview) throw new Error('showDocument opened preview tab');
 151 |     await until(() => count('textDocument/didOpen'), 'wire didOpen');
 152 |     const doc = vscode.window.activeTextEditor.document;
 153 |     result.cases.language = doc.languageId;
 154 |     await until(() => cdp('Runtime.evaluate', { expression: 'document.body.innerText' }).result.value.replace(/\s/g, ' ').includes('spike message'), 'painted editor');
 155 |     const otherUri = vscode.Uri.file(`${base}/repo/other.txt`);
```

### E2E exported functions and named cases (declaration/case source lines)


clients/vscode/test/e2e/head-operations.js:
`clients/vscode/test/e2e/head-operations.js:9: exports.run = async function run({ extension, evidence, repo, cdp, waitFor, checkpoint,`

clients/vscode/test/e2e/real-file-lifecycle.js:
`clients/vscode/test/e2e/real-file-lifecycle.js:8: exports.run = async function run({ root, evidence, repo, action, port }) {`

clients/vscode/test/e2e/real-file-lifecycle.mjs:
`clients/vscode/test/e2e/real-file-lifecycle.mjs:17: async function wait(check, label, base, timeout = 90000) {`
`clients/vscode/test/e2e/real-file-lifecycle.mjs:28: function processes() {`
`clients/vscode/test/e2e/real-file-lifecycle.mjs:55: write(`${base}/launcher/package.json`, { publisher: 'test', name: 'native-lifecycle', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './launcher.js', activationEvents: ['*'] });`
`clients/vscode/test/e2e/real-file-lifecycle.mjs:56: writeFileSync(`${base}/launcher/launcher.js`, `exports.activate = () => { setImmediate(() => require(${JSON.stringify(`${root}/clients/vscode/test/e2e/real-file-lifecycle.js`)}).run(${JSON.stringify({ root, evidence: base, repo, action, port })}).catch(e => require('fs').writeFileSync(${JSON.stringify(`${base}/failure.txt`)}, e.stack))); }; exports.deactivate = () => require('fs').appendFileSync(${JSON.stringify(`${base}/host.jsonl`)}, JSON.stringify({event:'deactivate',pid:process.pid,time:Date.now()})+'\\n');`);`

clients/vscode/test/e2e/real-file-native.js:
`clients/vscode/test/e2e/real-file-native.js:9: exports.run = async function run({ extension, evidence, repo, cdp, waitFor, checkpoint,`
`clients/vscode/test/e2e/real-file-native.js:76: cases.push({ name: 'fresh-run', runId: record.runId, initial });`
`clients/vscode/test/e2e/real-file-native.js:97: cases.push({ name: 'committed', runId: record.runId, before, after: head(), message: message() });`
`clients/vscode/test/e2e/real-file-native.js:151: cases.push({ name: 'native-cancel', head: head(), dirty: true, tabs: 1 });`
`clients/vscode/test/e2e/real-file-native.js:169: cases.push({ name: 'discard-preserves-disk-over-unsaved-sync', saved: message() });`
`clients/vscode/test/e2e/real-file-native.js:193: cases.push({ name: 'wrong-version-protocol-probe', runId: record.runId,`
`clients/vscode/test/e2e/real-file-native.js:212: cases.push({ name: 'unsafe-before-native-close', ...ownership(record) });`
`clients/vscode/test/e2e/real-file-native.js:232: cases.push({ name: 'unsafe-native-close-rejected', error: record.syncError,`
`clients/vscode/test/e2e/real-file-native.js:248: cases.push({ name: 'unsafe-restored-before-native-retry', ...ownership(record) });`
`clients/vscode/test/e2e/real-file-native.js:400: capabilities: {}, clientInfo: { name: 'R-EDIT-REAL semantic owner probe' } });`
`clients/vscode/test/e2e/real-file-native.js:417: cases.push({ name: 'two-repository-cross-wire-semantic-protocol-probes', roots: { A: repo, B: secondRepo },`
`clients/vscode/test/e2e/real-file-native.js:440: cases.push({ name: 'two-repository-native-A-last-close-B-pending', before: beforeProbes,`
`clients/vscode/test/e2e/real-file-native.js:449: cases.push({ name: 'two-repository-native-A-close-abort-B', beforeAbort,`
`clients/vscode/test/e2e/real-file-native.js:459: cases.push({ name: 'two-repository-late-native-B-close-after-abort', before: beforeAbort,`

clients/vscode/test/e2e/real-file-spike.js:
`clients/vscode/test/e2e/real-file-spike.js:14: function until(check, label, timeout = 5000) {`
`clients/vscode/test/e2e/real-file-spike.js:28: exports.activate = async function activate(context) {`
`clients/vscode/test/e2e/real-file-spike.js:73: if (process.env.MAGIT_REAL_FILE_NATIVE === '1') setImmediate(() => exports.run().catch(error => console.error(error)));`
`clients/vscode/test/e2e/real-file-spike.js:78: exports.deactivate = function deactivate() {`
`clients/vscode/test/e2e/real-file-spike.js:83: async function observeLifecycle(client) {`
`clients/vscode/test/e2e/real-file-spike.js:121: exports.run = async function run() {`

clients/vscode/test/e2e/real-file-spike.mjs:
`clients/vscode/test/e2e/real-file-spike.mjs:25: async function lifecycle() {`
`clients/vscode/test/e2e/real-file-spike.mjs:45: writeFileSync(`${dir}/extension/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));`
`clients/vscode/test/e2e/real-file-spike.mjs:121: writeFileSync(`${ext}/package.json`, JSON.stringify({ publisher: 'spike', name: 'real-file-spike', version: '1.0.0', engines: { vscode: '^1.85.0' }, main: './extension.js', activationEvents: ['*'] }));`

clients/vscode/test/e2e/run.mjs:
`clients/vscode/test/e2e/run.mjs:108: function ownedPids(base) {`
`clients/vscode/test/e2e/run.mjs:118: function stopOwned(base) {`
`clients/vscode/test/e2e/run.mjs:133: publisher: 'test', name: 'native-edit-launcher', version: '1.0.0',`
`clients/vscode/test/e2e/run.mjs:138: exports.activate = function () {`

clients/vscode/test/e2e/suite.js:
`clients/vscode/test/e2e/suite.js:21: function settle(milliseconds) {`
`clients/vscode/test/e2e/suite.js:32: async function waitFor(condition, label, timeout = 15000) {`
`clients/vscode/test/e2e/suite.js:47: function workspaceFiles(root) {`
`clients/vscode/test/e2e/suite.js:62: async function run() {`
`clients/vscode/test/e2e/suite.js:1721: cases.push({ name: 'native-save-normal-close', message: commitMessage, head: head() });`
`clients/vscode/test/e2e/suite.js:1744: cases.push({ name: 'version-recovery-confirm', head: head(),`
`clients/vscode/test/e2e/suite.js:1780: cases.push({ name: 'native-last-tab-saved-file-close', head: head() });`
`clients/vscode/test/e2e/suite.js:1792: cases.push({ name: 'progress-cancel', token, head: head() });`

### Explicit findings / limits
`window.dialogStyle` search: `rg -n "window\.dialogStyle" clients/vscode/test/e2e`; actual settings are shown in `run.mjs` (line 69) and lifecycle launcher (line 50); they set custom dialog style. Optional xvfb/CDP Save/Don't Save DOM experiment: NOT ATTEMPTED. `rg -n 'view/open|status/repo|viewId.*status|textDocument/didOpen' build/test-logs/vscode.*` returned no matches; existing build logs did not yield a real VSCode client→server status-open wire excerpt. `wire.jsonl`/`api.jsonl` references in spike sources are a configured/optional evidence path, not captured evidence found here. Actual trace excerpt: NOT FOUND. Do not infer from fixtures.


