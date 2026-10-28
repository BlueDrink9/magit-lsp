'use strict';
// Isolated test extension and extension-host suite; never loads production code.
const vscode = require('vscode');
const fs = require('fs');
const { execFileSync } = require('child_process');
const assert = require('node:assert/strict');
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
  }, { documentSelector: [{ scheme: 'file', pattern: `${base}/repo/.git/COMMIT_EDITMSG` }],
    middleware: { window: { showDocument: async (params, next) => {
      if (params.external) return next(params);
      fs.appendFileSync(`${base}/api.jsonl`, JSON.stringify({ event: 'showDocument', uri: params.uri, preview: false }) + '\n');
      // languageclient 9's default handler ignores a preview property on params.
      await vscode.window.showTextDocument(vscode.Uri.parse(params.uri), {
        preview: false, preserveFocus: params.takeFocus !== true,
        selection: params.selection && client.protocol2CodeConverter.asRange(params.selection),
      });
      return { success: true };
    } } },
  });
  context.subscriptions.push(vscode.workspace.onDidOpenTextDocument(d =>
    fs.appendFileSync(`${base}/api.jsonl`, JSON.stringify({ event: 'open', uri: d.uri.toString(), language: d.languageId }) + '\n')));
  context.subscriptions.push(vscode.workspace.onDidCloseTextDocument(d =>
    fs.appendFileSync(`${base}/api.jsonl`, JSON.stringify({ event: 'close', uri: d.uri.toString(), language: d.languageId }) + '\n')));
  context.subscriptions.push(vscode.window.tabGroups.onDidChangeTabs(event => {
    const uri = vscode.Uri.file(`${base}/repo/.git/COMMIT_EDITMSG`).toString();
    const matchingTabs = vscode.window.tabGroups.all.flatMap(group => group.tabs)
      .filter(tab => tab.input && tab.input.uri && tab.input.uri.toString() === uri).length;
    const closed = event.closed.map(tab => ({
      uri: tab.input && tab.input.uri ? tab.input.uri.toString() : null,
      isCommitEditMsg: !!(tab.input && tab.input.uri && tab.input.uri.toString() === uri),
    }));
    fs.appendFileSync(`${base}/api.jsonl`, JSON.stringify({ event: 'tabs', closed, matchingTabs,
      qualified: closed.some(tab => tab.isCommitEditMsg) && matchingTabs === 0,
      groups: vscode.window.tabGroups.all.map(group => ({ viewColumn: group.viewColumn,
        uris: group.tabs.map(tab => tab.input?.uri?.toString() || null) })),
    }) + '\n');
  }));
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
    const api = () => fs.existsSync(`${base}/api.jsonl`)
      ? fs.readFileSync(`${base}/api.jsonl`, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
    const qualifiedClosures = () => api().filter(item => item.event === 'tabs' && item.qualified);
    const matchingTabs = () => vscode.window.tabGroups.all.flatMap(g => g.tabs)
      .filter(tab => tab.input?.uri?.toString() === uri.toString());
    const show = () => client.sendRequest('spike/show', { uri: uri.toString(), takeFocus: true });
    result.cases.show = await show();
    assert.equal(result.cases.show.success, true);
    assert.equal(api().filter(item => item.event === 'showDocument').length, 1, 'real middleware invoked');
    result.cases.preview = { isPreview: vscode.window.tabGroups.all.flatMap(g => g.tabs)
      .find(tab => tab.input && tab.input.uri && tab.input.uri.toString() === uri.toString()).isPreview };
    if (result.cases.preview.isPreview) throw new Error('showDocument opened preview tab');
    await until(() => count('textDocument/didOpen'), 'wire didOpen');
    const doc = vscode.window.activeTextEditor.document;
    result.cases.language = doc.languageId;
    await until(() => cdp('Runtime.evaluate', { expression: 'document.body.innerText' }).result.value.replace(/\s/g, ' ').includes('spike message'), 'painted editor');
    shot('01-open');
    const otherUri = vscode.Uri.file(`${base}/repo/other.txt`);
    fs.writeFileSync(otherUri.fsPath, 'unrelated file: switching is not closure\n');
    await vscode.window.showTextDocument(otherUri, { preview: false });
    await until(() => vscode.window.activeTextEditor?.document.uri.toString() === otherUri.toString(), 'switched away');
    assert.equal(matchingTabs().length, 1);
    assert.equal(qualifiedClosures().length, 0, 'switching away must not qualify');
    await show();
    assert.equal(qualifiedClosures().length, 0, 'switching back must not qualify');
    result.cases.focusSwitch = { qualifiedClosureDelta: 0, matchingTabs: matchingTabs().length };
    await vscode.commands.executeCommand('workbench.action.splitEditorRight');
    await until(() => matchingTabs().length === 2, 'same URI in two groups');
    assert.equal(vscode.window.tabGroups.all.filter(g => g.tabs.some(tab => tab.input?.uri?.toString() === uri.toString())).length, 2);
    shot('01b-two-groups');
    await vscode.commands.executeCommand('workbench.action.focusFirstEditorGroup');
    await show();
    const eventsBefore = api().filter(item => item.event === 'tabs' && item.closed.some(tab => tab.isCommitEditMsg)).length;
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await until(() => matchingTabs().length === 1 && api().filter(item => item.event === 'tabs' && item.closed.some(tab => tab.isCommitEditMsg)).length > eventsBefore, 'nonlast close observed');
    assert.equal(qualifiedClosures().length, 0, 'tab in another group prevents qualification');
    result.cases.nonlastClose = { matchingTabs: matchingTabs().length, qualifiedClosureDelta: 0 };
    shot('01c-nonlast-close');
    await vscode.commands.executeCommand('workbench.action.focusNextGroup');
    await show();
    assert.equal(matchingTabs().length, 1, 'show focuses existing remaining tab');
    const before = count('textDocument/didClose');
    const qualifiedBefore = qualifiedClosures().length;
    const start = Date.now();
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await until(() => qualifiedClosures().length > qualifiedBefore, 'last matching tab closed');
    assert.equal(qualifiedClosures().length - qualifiedBefore, 1, 'last close qualifies exactly once');
    assert.equal(matchingTabs().length, 0);
    result.cases.nativeClose = { elapsedMs: Date.now() - start, tabCount: vscode.window.tabGroups.all.flatMap(g => g.tabs).length,
      documentClosed: doc.isClosed, wireCloseDelta: count('textDocument/didClose') - before,
      qualifiedClosureDelta: qualifiedClosures().length - qualifiedBefore };
    result.cases.didCloseObservation = { delta: count('textDocument/didClose') - before,
      supportedFallbackWithoutDidClose: count('textDocument/didClose') === before };
    shot('02-native-close');
    // Unrelated closed events while the draft is absent must not qualify.
    const otherTab = vscode.window.tabGroups.all.flatMap(g => g.tabs).find(tab => tab.input?.uri?.toString() === otherUri.toString());
    assert.ok(otherTab);
    assert.equal(await vscode.window.tabGroups.close(otherTab), true);
    await until(() => api().some(item => item.event === 'tabs' && item.closed.some(tab => tab.uri === otherUri.toString())), 'unrelated close observed');
    assert.equal(qualifiedClosures().length, qualifiedBefore + 1);
    result.cases.unrelatedClose = { qualifiedClosureDelta: 0 };
    await show();
    await until(() => matchingTabs().length === 1, 'reopened tab');
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
    result.cases.dirtyCancel = { stillOpen: !editor.document.isClosed, dirty: editor.document.isDirty,
      matchingTabs: matchingTabs().length, qualifiedClosureDelta: qualifiedClosures().length - qualifiedBefore - 1 };
    assert.equal(result.cases.dirtyCancel.stillOpen, true);
    assert.equal(result.cases.dirtyCancel.dirty, true);
    assert.equal(result.cases.dirtyCancel.matchingTabs, 1);
    assert.equal(result.cases.dirtyCancel.qualifiedClosureDelta, 0);
    shot('03b-dirty-cancel');
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
    const languageClosures = qualifiedClosures().length;
    await vscode.languages.setTextDocumentLanguage(editor.document, 'plaintext');
    await until(() => count('textDocument/didOpen') > opens && count('textDocument/didClose') > closes, 'language change close/open');
    result.cases.languageChange = { openDelta: count('textDocument/didOpen') - opens, closeDelta: count('textDocument/didClose') - closes };
    result.cases.languageChange.qualifiedClosureDelta = qualifiedClosures().length - languageClosures;
    result.cases.languageChange.matchingTabs = matchingTabs().length;
    assert.equal(result.cases.languageChange.qualifiedClosureDelta, 0, 'language change must not qualify');
    assert.equal(result.cases.languageChange.matchingTabs, 1);
    assert.equal(qualifiedClosures().length, 1, 'only the last draft tab close qualified in the entire run');
    shot('04-refresh-language');
    result.pending = ['native dirty Save-on-close', 'native dirty Discard-on-close', 'shutdown', 'reload', 'workspace-change'];
    result.gate = 'pass';
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
