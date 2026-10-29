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
  const index = () => git('show', ':tracked.txt');
  const worktree = () => fs.readFileSync(`${repo}/tracked.txt`, 'utf8');
  const message = () => git('log', '-1', '--format=%B');
  const tabs = (record) => vscode.window.tabGroups.all.flatMap((group) => group.tabs)
    .filter((tab) => tab.input?.uri?.toString() === record.uri.toString());
  const confirmations = (record) => requests.filter((request) =>
    request.method === 'workspace/executeCommand'
    && request.params.arguments?.[0]?.runId === record.runId
    && request.params.arguments[0].command === 'common.confirm'
    && request.params.arguments[0].source === 'saved-file');
  const status = vscode.window.activeTextEditor;
  assert.equal(status?.document.uri.scheme, 'magit', 'shared production status required');
  const cases = [];
  const screenshots = [];
  const events = [];
  const identities = new WeakMap();
  let nextIdentity = 0;
  const identity = (object) => {
    if (!identities.has(object)) identities.set(object, ++nextIdentity);
    return identities.get(object);
  };
  const tabState = (tab) => ({ id: identity(tab), uri: tab.input?.uri?.toString() });
  const ownership = (record) => ({ runId: record.runId,
    document: identity(record.document), documentClosed: record.document.isClosed,
    nativeClosed: record.nativeClosed, owned: [...record.tabs].map(tabState),
    actual: tabs(record).map(tabState) });
  const capture = (name) => { screenshots.push(name); shot(name); };
  const observer = vscode.window.tabGroups.onDidChangeTabs((event) => events.push({
    closed: event.closed.map(tabState),
    groups: vscode.window.tabGroups.all.map((group) => group.tabs.map(tabState)),
    records: [...store.drafts.values()].filter((record) => record.document && record.tabs).map(ownership),
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
  const completed = async (record, before, expected, attempts = 1) => {
    await waitFor(() => head() !== before && !store.drafts.has(record.runId), 'native close commits and forgets');
    assert.equal(message(), expected);
    assert.equal(git('rev-list', '--count', `${before}..HEAD`), '1', 'one helper release, one commit');
    assert.equal(confirmations(record).length, attempts, 'one confirmation per actual last-tab close');
    assert.equal(confirmations(record)[0].params.arguments[0].source, 'saved-file');
    assert(confirmations(record).every((request) => request.params.arguments[0].version === record.savedVersion),
      'native close names the exact saved version');
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

    // Adversarial executeCommand probes exercise the live server protocol; they
    // are deliberately separate from the real tab-close events below.
    record = await start();
    before = head();
    const oldIndex = index();
    const oldWorktree = worktree();
    const saved = 'native wrong-version saved bytes\n';
    await replace(record, saved);
    await record.document.save();
    await record.queue;
    const liveText = record.document.getText();
    const wrongVersion = await store.client.sendRequest('workspace/executeCommand', {
      command: 'magit.command', arguments: [{ command: 'common.confirm', runId: record.runId,
        uri: record.uri.toString(), version: record.version - 1, source: 'synchronized' }],
    }).catch((error) => ({ error: String(error) }));
    assert.match(wrongVersion.error, /exact synchronized commit message version/);
    assert.equal(head(), before, 'wrong-version protocol probe cannot commit');
    assert.equal(index(), oldIndex);
    assert.equal(worktree(), oldWorktree);
    assert.equal(fs.readFileSync(record.uri.fsPath, 'utf8'), saved, 'probe cannot overwrite saved bytes');
    assert.equal(record.document.getText(), liveText);
    assert(store.drafts.has(record.runId));
    cases.push({ name: 'wrong-version-protocol-probe', runId: record.runId,
      version: record.version, result: wrongVersion, head: head(), index: index(),
      worktree: worktree(), saved: fs.readFileSync(record.uri.fsPath, 'utf8'), liveText });
    capture(`real-file-native-${sequence}-wrong-version-retained.png`);
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await completed(record, before, saved.trimEnd());

    // Unsafe saved-file close: a dangling symlink avoids conflating the native
    // editor's following/reloading an existing target with the server's secure read.
    record = await start();
    before = head();
    const runId = record.runId;
    await replace(record, 'native unsafe-file recovery message\n\nretained body\n');
    await record.document.save();
    await record.queue;
    const originalBytes = fs.readFileSync(record.uri.fsPath);
    const unsafeIndex = index();
    const unsafeWorktree = worktree();
    const unsafeVersion = record.version;
    cases.push({ name: 'unsafe-before-native-close', ...ownership(record) });
    const sentinel = `${repo}/unsafe-sentinel.txt`;
    fs.writeFileSync(sentinel, 'sentinel must remain untouched\n');
    const missingTarget = `${repo}/missing-unsafe-target.txt`;
    fs.unlinkSync(record.uri.fsPath);
    fs.symlinkSync(missingTarget, record.uri.fsPath);
    const closing = vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await waitFor(() => store.drafts.get(runId)?.nativeClosed, 'native unsafe-file close observed');
    await closing;
    await waitFor(() => confirmations(record).length === 1, 'unsafe saved-file close confirmation request');
    await record.queue;
    assert.match(record.syncError, /Cannot open commit draft target/);
    assert(fs.lstatSync(record.uri.fsPath).isSymbolicLink());
    assert(!fs.existsSync(missingTarget), 'secure read does not create the symlink target');
    assert.equal(head(), before);
    assert.equal(index(), unsafeIndex);
    assert.equal(worktree(), unsafeWorktree);
    assert.equal(store.drafts.get(runId), record, 'rejection retains the same pending run');
    assert.equal(record.version, unsafeVersion);
    assert.equal(record.text, originalBytes.toString());
    cases.push({ name: 'unsafe-native-close-rejected', error: record.syncError,
      head: head(), index: index(), worktree: worktree(), version: record.version,
      text: record.text, ...ownership(record) });
    capture(`real-file-native-${sequence}-unsafe-rejected.png`);
    // VS Code may evict the document after its file disappears. Restore the
    // Git target before asking the existing adapter to present the retained run.
    fs.unlinkSync(record.uri.fsPath);
    fs.writeFileSync(record.uri.fsPath, originalBytes);
    await vscode.window.showTextDocument(record.uri, { preview: false });
    await waitFor(() => tabs(record).length === 1, 'retained draft presented after target repair');
    assert(record.tabs.has(tabs(record)[0]), 'retry tab identity belongs to the retained run');
    assert.equal(head(), before);
    assert(store.drafts.has(runId));
    assert.equal(record.runId, runId);
    assert.equal(fs.readFileSync(record.uri.fsPath, 'utf8'), originalBytes.toString());
    assert.equal(fs.readFileSync(sentinel, 'utf8'), 'sentinel must remain untouched\n');
    cases.push({ name: 'unsafe-restored-before-native-retry', ...ownership(record) });
    capture(`real-file-native-${sequence}-unsafe-recovered.png`);
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await completed(record, before, originalBytes.toString().trimEnd(), 2);
    assert.equal(git('rev-list', '--count', `${before}..HEAD`), '1');
    assert.equal(fs.readFileSync(sentinel, 'utf8'), 'sentinel must remain untouched\n');
    fs.unlinkSync(sentinel);

    // Semantic duplicate/stale requests are protocol probes, not fabricated
    // native duplicate tab events. A new run reuses the same real Git file.
    const terminal = record;
    const probe = async (name, active) => {
      const snapshot = { head: head(), index: index(), worktree: worktree(),
        disk: fs.readFileSync(terminal.uri.fsPath, 'utf8'), liveText: active?.document.getText() };
      const confirmResult = await store.client.sendRequest('workspace/executeCommand', {
        command: 'magit.command', arguments: [{ command: 'common.confirm', runId: terminal.runId,
          uri: terminal.uri.toString(), version: terminal.version, source: 'synchronized' }],
      }).catch((error) => ({ error: String(error) }));
      assert.notEqual(confirmResult?.type, 'confirmed');
      const abortResult = await store.client.sendRequest('view/invoke', {
        viewId: terminal.baseViewId, command: 'common.cancel', args: { runId: terminal.runId },
        cursor: { line: 0, column: 0 },
      }).catch((error) => ({ error: String(error) }));
      assert.equal(head(), snapshot.head);
      assert.equal(index(), snapshot.index);
      assert.equal(worktree(), snapshot.worktree);
      assert.equal(fs.readFileSync(terminal.uri.fsPath, 'utf8'), snapshot.disk);
      if (active) {
        assert.equal(store.drafts.get(active.runId), active);
        assert.equal(active.document.getText(), snapshot.liveText);
        assert.equal(tabs(active).length, 1);
      } else assert.equal(store.drafts.size, 0);
      cases.push({ name, oldRunId: terminal.runId, activeRunId: active?.runId,
        snapshot, confirmResult, abortResult });
    };
    await probe('duplicate-terminal-confirm-abort-protocol-probes');
    record = await start();
    before = head();
    assert.equal(record.uri.toString(), terminal.uri.toString());
    await replace(record, 'native same-URI run survives stale protocol probes\n');
    await record.document.save();
    await record.queue;
    await probe('stale-old-run-confirm-abort-after-URI-reuse-protocol-probes', record);
    capture(`real-file-native-${sequence}-stale-probes-retained.png`);
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await completed(record, before, 'native same-URI run survives stale protocol probes');
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
