'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const vscode = require('vscode');

/** Exercise production real-file drafts with actual workbench dirty dialogs. */
exports.run = async function run({ extension, evidence, repo, cdp, waitFor, shot,
  requests, draftNotes }) {
  const { store } = require(`${extension.extensionPath}/lib/store`);
  const git = (...args) => execFileSync('git', ['-C', repo, ...args], { encoding: 'utf8' }).trim();
  const head = () => git('rev-parse', 'HEAD');
  const message = () => git('log', '-1', '--format=%B');
  const tabs = (record) => vscode.window.tabGroups.all.flatMap((group) => group.tabs)
    .filter((tab) => tab.input?.uri?.toString() === record.uri.toString());
  const confirmations = (record) => requests.filter((request) =>
    request.method === 'workspace/executeCommand'
    && request.params.arguments?.[0]?.runId === record.runId
    && request.params.arguments[0].command === 'common.confirm');
  const status = vscode.window.activeTextEditor;
  assert.equal(status?.document.uri.scheme, 'magit', 'shared production status required');
  const cases = [];
  const screenshots = [];
  const events = [];
  const capture = (name) => { screenshots.push(name); shot(name); };
  const observer = vscode.window.tabGroups.onDidChangeTabs((event) => events.push({
    closed: event.closed.map((tab) => tab.input?.uri?.toString()),
    groups: vscode.window.tabGroups.all.map((group) => group.tabs.map((tab) => tab.input?.uri?.toString())),
  }));
  const persist = () => fs.writeFileSync(`${evidence}/real-file-native-trace.json`, JSON.stringify({
    cases, events, requests, notes: draftNotes, screenshots,
  }, null, 2));
  const key = (key, code, windowsVirtualKeyCode) => {
    for (const type of ['keyDown', 'keyUp']) {
      cdp('Input.dispatchKeyEvent', { type, key, code, windowsVirtualKeyCode });
    }
  };
  const dialog = () => cdp('Runtime.evaluate', { expression: 'document.body.innerText' }).result.value
    .includes('Do you want to save');
  let previous;
  let sequence = 0;
  const start = async () => {
    await waitFor(() => store.drafts.size === 0, 'previous production draft forgotten');
    await vscode.window.showTextDocument(status.document, { preview: false });
    fs.writeFileSync(`${repo}/tracked.txt`, `native close fixture ${++sequence}\n`);
    git('add', 'tracked.txt');
    await vscode.commands.executeCommand('magit.invoke', { command: 'commit.create' });
    await waitFor(() => [...store.drafts.values()].some((draft) => draft.attached), 'production Git draft');
    const record = [...store.drafts.values()][0];
    assert.equal(record.uri.scheme, 'file');
    assert.equal(tabs(record).length, 1);
    assert.equal(tabs(record)[0].isPreview, false);
    const initial = record.document.getText();
    assert.equal(initial, fs.readFileSync(record.uri.fsPath, 'utf8'), 'new run opens fresh Git disk text');
    if (previous) {
      assert.notEqual(record.runId, previous.runId);
      assert(!initial.includes(previous.message), 'prior message is not reused');
    }
    cases.push({ name: 'fresh-run', runId: record.runId, initial });
    return record;
  };
  const replace = async (record, text) => {
    const editor = await vscode.window.showTextDocument(record.uri, { preview: false });
    const document = editor.document;
    assert(await editor.edit((edit) => edit.replace(new vscode.Range(document.positionAt(0),
      document.positionAt(document.getText().length)), text)));
    await waitFor(() => record.text === text && !record.flushQueued, 'production full-sync text');
    return document;
  };
  const completed = async (record, before, expected) => {
    await waitFor(() => head() !== before && !store.drafts.has(record.runId), 'native close commits and forgets');
    assert.equal(message(), expected);
    assert.equal(git('rev-list', '--count', `${before}..HEAD`), '1', 'one helper release, one commit');
    assert.equal(confirmations(record).length, 1, 'only last tab sends one confirmation');
    assert.equal(confirmations(record)[0].params.arguments[0].source, 'saved-file');
    assert.equal(tabs(record).length, 0);
    previous = { runId: record.runId, message: expected };
    cases.push({ name: 'committed', runId: record.runId, before, after: head(), message: message() });
    capture(`real-file-native-${sequence}-committed.png`);
    persist();
  };
  const closeDialog = async (record, action) => {
    const closing = vscode.commands.executeCommand('magit.closeEdit');
    await waitFor(dialog, `actual native ${action} dialog`);
    assert(record.document.isDirty);
    capture(`real-file-native-${sequence}-${action}-dialog.png`);
    if (action === 'cancel') key('Escape', 'Escape', 27);
    else {
      if (action === 'discard') key('Tab', 'Tab', 9);
      key('Enter', 'Enter', 13);
    }
    await closing;
    await waitFor(() => !dialog(), `native ${action} dialog dismissed`);
  };
  try {
    let record = await start();
    let before = head();
    await replace(record, 'native saved message\n\nexact body\n');
    await record.document.save();
    await record.queue;
    assert.equal(head(), before, 'save alone never commits');
    assert.equal(confirmations(record).length, 0);
    capture('real-file-native-save-alone.png');
    await vscode.window.showTextDocument(status.document, { preview: false });
    assert.equal(head(), before, 'focus/switch never commits');
    await vscode.window.showTextDocument(record.uri, { preview: false });
    const document = await vscode.languages.setTextDocumentLanguage(record.document, 'plaintext');
    await vscode.window.showTextDocument(document, { preview: false });
    assert.equal(head(), before, 'language-change document close never commits');
    assert.equal(confirmations(record).length, 0);
    await vscode.commands.executeCommand('workbench.action.splitEditorRight');
    await waitFor(() => tabs(record).length === 2, 'same draft in two real groups');
    capture('real-file-native-two-groups.png');
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await waitFor(() => tabs(record).length === 1, 'nonlast tab closed');
    assert.equal(head(), before, 'nonlast close never commits');
    assert.equal(confirmations(record).length, 0);
    capture('real-file-native-nonlast-close.png');
    await vscode.window.showTextDocument(record.uri, { preview: false });
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await completed(record, before, 'native saved message\n\nexact body');

    record = await start();
    before = head();
    await replace(record, 'native Save new bytes\n\nnew body\n');
    await closeDialog(record, 'cancel');
    assert.equal(head(), before);
    assert.equal(tabs(record).length, 1);
    assert(record.document.isDirty);
    assert(store.drafts.has(record.runId));
    assert.equal(confirmations(record).length, 0);
    cases.push({ name: 'native-cancel', head: head(), dirty: true, tabs: 1 });
    capture('real-file-native-cancel-retained.png');
    await closeDialog(record, 'save');
    await completed(record, before, 'native Save new bytes\n\nnew body');

    record = await start();
    before = head();
    await replace(record, 'native PREVIOUS saved bytes\n\nold body\n');
    await record.document.save();
    await record.queue;
    await replace(record, 'native UNSAVED must never commit\n');
    assert(draftNotes.some((note) => note.method === 'textDocument/didChange'
      && note.params.textDocument.uri === record.uri.toString()
      && note.params.contentChanges[0].text === 'native UNSAVED must never commit\n'));
    assert.equal(fs.readFileSync(record.uri.fsPath, 'utf8'), 'native PREVIOUS saved bytes\n\nold body\n');
    assert.equal(head(), before);
    await closeDialog(record, 'discard');
    await completed(record, before, 'native PREVIOUS saved bytes\n\nold body');
    cases.push({ name: 'discard-preserves-disk-over-unsaved-sync', saved: message() });
    persist();
    fs.writeFileSync(`${evidence}/real-file-native-evidence.json`, JSON.stringify({
      gate: 'R-EDIT-REAL production native close', status: 'pass', cases, screenshots,
      trace: 'real-file-native-trace.json',
    }, null, 2));
  } finally {
    observer.dispose();
    persist();
  }
};
