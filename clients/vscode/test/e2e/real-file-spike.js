'use strict';
// Isolated test extension and extension-host suite; never loads production code.
const vscode = require('vscode');
const fs = require('fs');
const { execFileSync } = require('child_process');
const base = process.env.MAGIT_REAL_FILE_EVIDENCE;
const root = process.env.MAGIT_REAL_FILE_ROOT;
const trace = () => fs.existsSync(`${base}/wire.jsonl`)
  ? fs.readFileSync(`${base}/wire.jsonl`, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
const count = (method) => trace().filter(m => m.method === method).length;

/** Await a condition with a bounded deadline, not a fixed settling delay. */
function until(check, label, timeout = 5000) {
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + timeout;
    const timer = setInterval(() => {
      try {
        const result = check();
        if (result) { clearInterval(timer); resolve(result); }
        else if (Date.now() >= deadline) { clearInterval(timer); reject(new Error(`Deadline: ${label}`)); }
      } catch (error) { clearInterval(timer); reject(error); }
    }, 25);
  });
}

/** Attach the actual languageclient to real files in the disposable repository. */
exports.activate = async function activate(context) {
  const { LanguageClient } = require(`${root}/clients/vscode/node_modules/vscode-languageclient/node`);
  const client = new LanguageClient('real-file-spike', 'Real file observer', {
    command: 'python3', args: [`${root}/tests/spikes/real_file_lsp.py`, '--record', `${base}/wire.jsonl`],
  }, { documentSelector: [{ scheme: 'file', pattern: `${base}/repo/.git/COMMIT_EDITMSG` }] });
  context.subscriptions.push(vscode.workspace.onDidOpenTextDocument(d =>
    fs.appendFileSync(`${base}/api.jsonl`, JSON.stringify({ event: 'open', uri: d.uri.toString(), language: d.languageId }) + '\n')));
  context.subscriptions.push(vscode.workspace.onDidCloseTextDocument(d =>
    fs.appendFileSync(`${base}/api.jsonl`, JSON.stringify({ event: 'close', uri: d.uri.toString(), language: d.languageId }) + '\n')));
  await client.start();
  if (process.env.MAGIT_REAL_FILE_NATIVE === '1') setImmediate(() => exports.run().catch(error => console.error(error)));
  return { client };
};

/** Exercise the real editor, retaining observations even when the hard gate fails. */
exports.run = async function run() {
  const result = { gate: 'incomplete', cases: {} };
  let client;
  const cdp = (method, params = {}) => JSON.parse(execFileSync(process.execPath, [
    `${root}/clients/vscode/test/e2e/real-file-spike.mjs`, '--cdp', process.env.MAGIT_REAL_FILE_CDP, method, JSON.stringify(params),
  ], { env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' }, encoding: 'utf8', timeout: 10000 }));
  const shot = name => fs.writeFileSync(`${base}/${name}.png`, Buffer.from(cdp('Page.captureScreenshot', { format: 'png' }).data, 'base64'));
  try {
    ({ client } = await vscode.extensions.getExtension('spike.real-file-spike').activate());
    const windowId = await until(() => {
      try {
        const ids = execFileSync('xdotool', ['search', '--name', base], { encoding: 'utf8' }).trim().split('\n');
        return ids.length === 1 && ids[0];
      } catch (_) { return false; }
    }, 'isolated window');
    try { execFileSync('bspc', ['node', windowId, '-t', 'floating'], { stdio: 'ignore' }); } catch (_) { /* other WM */ }
    execFileSync('xdotool', ['windowsize', '--sync', windowId, '1200', '760']);
    await until(() => cdp('Runtime.evaluate', { expression: 'innerWidth >= 1100' }).result.value, 'window geometry');
    const uri = vscode.Uri.file(`${base}/repo/.git/COMMIT_EDITMSG`);
    const show = () => client.sendRequest('spike/show', { uri: uri.toString(), takeFocus: true });
    result.cases.show = await show();
    await until(() => count('textDocument/didOpen'), 'wire didOpen');
    const doc = vscode.window.activeTextEditor.document;
    result.cases.language = doc.languageId;
    await until(() => cdp('Runtime.evaluate', { expression: 'document.body.innerText' }).result.value.replace(/\s/g, ' ').includes('spike message'), 'painted editor');
    shot('01-open');
    const before = count('textDocument/didClose');
    const start = Date.now();
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    try {
      await until(() => count('textDocument/didClose') > before, 'native tab close wire didClose');
      result.gate = 'pass';
    } catch (error) { result.gate = 'observed-fail'; result.closeError = error.message; }
    result.cases.nativeClose = { elapsedMs: Date.now() - start, tabCount: vscode.window.tabGroups.all.flatMap(g => g.tabs).length,
      documentClosed: doc.isClosed, wireCloseDelta: count('textDocument/didClose') - before };
    shot('02-native-close');
    await show();
    await until(() => vscode.window.tabGroups.all.some(g => g.tabs.length), 'reopened tab');
    const editor = vscode.window.activeTextEditor;
    result.cases.dirtyEdit = await editor.edit(edit => edit.insert(new vscode.Position(0, 0), 'dirty native text\n'));
    await until(() => editor.document.isDirty, 'dirty editor');
    if (process.env.MAGIT_REAL_FILE_NATIVE === '1') {
    let closeSettled = false;
    const closing = vscode.commands.executeCommand('workbench.action.closeActiveEditor').then(() => { closeSettled = true; });
    await until(() => cdp('Runtime.evaluate', { expression: 'document.body.innerText' }).result.value.includes('Do you want to save'), 'native dirty save dialog');
    shot('03-dirty-dialog');
    result.cases.dirtyDialog = { dirty: editor.document.isDirty, closeSettled };
    cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
    cdp('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
    await closing;
    result.cases.dirtyCancel = { stillOpen: !editor.document.isClosed, dirty: editor.document.isDirty };
    } else {
      result.cases.dirtyDialog = 'not tested here: extensionTestsPath suppresses native save confirmation; separate normal host required';
    }
    await editor.document.save();
    await until(() => count('textDocument/didSave'), 'save wire');
    fs.writeFileSync(uri.fsPath, 'replacement template\n');
    await show();
    result.cases.reusedShow = { text: editor.document.getText(), dirty: editor.document.isDirty };
    const refresh = await client.sendRequest('spike/apply', { edit: { documentChanges: [{
      textDocument: { uri: uri.toString(), version: editor.document.version },
      edits: [{ range: { start: { line: 0, character: 0 }, end: { line: editor.document.lineCount, character: 0 } }, newText: 'server refreshed template\n' }],
    }] } });
    result.cases.applyEdit = { response: refresh, text: editor.document.getText(), dirty: editor.document.isDirty };
    const opens = count('textDocument/didOpen'), closes = count('textDocument/didClose');
    await vscode.languages.setTextDocumentLanguage(editor.document, 'plaintext');
    await until(() => count('textDocument/didOpen') > opens && count('textDocument/didClose') > closes, 'language change close/open');
    result.cases.languageChange = { openDelta: count('textDocument/didOpen') - opens, closeDelta: count('textDocument/didClose') - closes };
    shot('04-refresh-language');
  } catch (error) {
    result.error = error.stack;
    try { fs.writeFileSync(`${base}/failure-dom.txt`, cdp('Runtime.evaluate', { expression: 'document.body.innerText' }).result.value); } catch (_) { /* retain error */ }
    try { shot('failure'); } catch (_) { /* retain original failure */ }
  } finally {
    fs.writeFileSync(`${base}/result.json`, JSON.stringify(result, null, 2));
    if (client) await client.stop();
  }
  if (result.error || result.gate !== 'pass') throw new Error(JSON.stringify(result));
};
