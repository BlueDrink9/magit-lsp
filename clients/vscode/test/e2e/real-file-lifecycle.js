'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const vscode = require('vscode');

/** Observe production before activation and drive one native lifecycle transition. */
exports.run = async function run({ root, evidence, repo, action, port }) {
  const record = data => fs.appendFileSync(`${evidence}/host.jsonl`, JSON.stringify({ time: Date.now(), pid: process.pid, ...data }) + '\n');
  const write = (name, data) => fs.writeFileSync(`${evidence}/${name}.json`, JSON.stringify(data, null, 2));
  const wait = async (check, label) => {
    const deadline = Date.now() + 30000;
    while (!await check()) {
      if (Date.now() > deadline) throw Error(`Deadline: ${label}`);
      await new Promise(resolve => setTimeout(resolve, 25));
    }
  };
  const shot = name => {
    const result = JSON.parse(execFileSync('node', [`${root}/clients/vscode/test/e2e/run.mjs`, '--cdp', String(port), 'Page.captureScreenshot', '{"format":"png"}'], { encoding: 'utf8' }));
    fs.writeFileSync(`${evidence}/${name}.png`, Buffer.from(result.data, 'base64'));
  };
  record({ event: 'activate', workspace: vscode.workspace.workspaceFolders?.map(f => f.uri.toString()) });
  const { LanguageClient } = require(`${root}/clients/vscode/node_modules/vscode-languageclient/node`);
  for (const method of ['sendRequest', 'sendNotification']) {
    const original = LanguageClient.prototype[method];
    LanguageClient.prototype[method] = function (type, params, ...rest) {
      record({ event: method, method: typeof type === 'string' ? type : type.method, params });
      return original.call(this, type, params, ...rest);
    };
  }
  const onProgress = LanguageClient.prototype.onProgress;
  LanguageClient.prototype.onProgress = function (type, token, handler) {
    return onProgress.call(this, type, token, value => { record({ event: 'progress', token, value }); return handler(value); });
  };
  vscode.window.tabGroups.onDidChangeTabs(event => record({ event: 'tabs', closed: event.closed.map(t => t.input?.uri?.toString()), open: vscode.window.tabGroups.all.flatMap(g => g.tabs).map(t => t.input?.uri?.toString()) }));
  const extension = vscode.extensions.getExtension('magit-lsp.magit-lsp');
  assert(extension, 'production extension required');
  await extension.activate();
  // Keep the native editor readable even under a small tiled desktop window.
  await vscode.commands.executeCommand('workbench.action.closeSidebar');
  await vscode.commands.executeCommand('workbench.action.closeAuxiliaryBar');
  await vscode.commands.executeCommand('workbench.action.closePanel');
  const { store } = require(`${root}/clients/vscode/lib/store`);
  const old = fs.existsSync(`${evidence}/action.json`) ? JSON.parse(fs.readFileSync(`${evidence}/action.json`)) : undefined;
  if (old) {
    assert.equal(fs.readFileSync(new URL(old.uri), 'utf8'), old.text, 'saved file survived native action');
    shot('after');
    write('resumed', { workspace: vscode.workspace.workspaceFolders?.map(f => f.uri.toString()), pid: process.pid });
    if (action !== 'reload-window') return;
  }
  await vscode.commands.executeCommand('magit.open');
  await wait(() => vscode.window.activeTextEditor?.document.uri.scheme === 'magit', 'production status');
  await vscode.commands.executeCommand('magit.invoke', { command: 'commit.create' });
  await wait(() => [...store.drafts.values()].some(d => d.attached), 'production draft');
  const draft = [...store.drafts.values()][0];
  assert.equal(draft.uri.scheme, 'file');
  const initial = draft.document.getText();
  assert.equal(initial, fs.readFileSync(draft.uri.fsPath, 'utf8'), 'fresh Git file');
  if (old) {
    assert.notEqual(draft.runId, old.runId);
    assert.equal(draft.uri.toString(), old.uri, 'same URI reused');
    assert(!initial.includes(old.text.trim()), 'old message not reused');
    let stale;
    try {
      stale = await store.client.sendRequest('workspace/executeCommand', { command: 'magit.command', arguments: [{ command: 'common.confirm', runId: old.runId, uri: old.uri, version: old.version, source: 'saved-file' }] });
    } catch (error) { stale = { error: String(error) }; }
    assert(store.drafts.has(draft.runId), 'stale confirmation cannot remove new run');
    assert.equal(execFileSync('git', ['-C', repo, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), old.head);
    assert.notEqual(stale?.type, 'confirmed');
    await require(`${root}/clients/vscode/lib/drafts`).cancel(draft);
    write('fresh', { runId: draft.runId, uri: draft.uri.toString(), initial, stale });
    return;
  }
  const text = `Saved meaningful lifecycle ${action}\n\nPreserve this body.\n`;
  const editor = await vscode.window.showTextDocument(draft.uri, { preview: false });
  assert(await editor.edit(edit => edit.replace(new vscode.Range(draft.document.positionAt(0), draft.document.positionAt(initial.length)), text)));
  await wait(() => draft.text === text && !draft.flushQueued, 'full sync');
  assert(await draft.document.save());
  await draft.queue;
  assert(!draft.document.isDirty);
  assert.equal(fs.readFileSync(draft.uri.fsPath, 'utf8'), text);
  editor.revealRange(new vscode.Range(0, 0, 0, 0), vscode.TextEditorRevealType.AtTop);
  await wait(() => {
    const rendered = JSON.parse(execFileSync('node', [`${root}/clients/vscode/test/e2e/run.mjs`, '--cdp', String(port), 'Runtime.evaluate', JSON.stringify({ expression: 'document.body.innerText' })], { encoding: 'utf8' }));
    return rendered.result.value.replace(/\s+/g, ' ').includes(`Saved meaningful lifecycle ${action}`);
  }, 'saved message painted in native editor');
  shot('before');
  write('ready', { hostPid: process.pid, runId: draft.runId, uri: draft.uri.toString(), version: draft.version, text, head: execFileSync('git', ['-C', repo, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() });
  await wait(() => fs.existsSync(`${evidence}/go.json`), 'supervisor process census');
  write('action', JSON.parse(fs.readFileSync(`${evidence}/ready.json`)));
  record({ event: 'action', action });
  try {
    if (action === 'close-window') await vscode.commands.executeCommand('workbench.action.closeWindow');
    else if (action === 'reload-window') await vscode.commands.executeCommand('workbench.action.reloadWindow');
    else await vscode.commands.executeCommand('vscode.openFolder', vscode.Uri.file(`${evidence}/other-workspace`), false);
  } catch (error) {
    // Native shutdown cancels outstanding extension-host RPC promises. This is
    // an observation, not completion: the supervisor still requires exit/restart.
    record({ event: 'action-rpc-rejected', error: String(error) });
    if (error.name !== 'Canceled' && error.message !== 'Canceled') throw error;
  }
};
