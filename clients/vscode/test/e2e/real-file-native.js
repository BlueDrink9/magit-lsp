'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const vscode = require('vscode');

/** Exercise production real-file drafts with actual workbench dirty dialogs. */
exports.run = async function run({ extension, evidence, repo, cdp, waitFor, checkpoint,
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
  const views = require(`${extension.extensionPath}/lib/views`);
  const cases = [];
  const checkpoints = [];
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
  const capture = (name) => { checkpoints.push(name); checkpoint(name); };
  const observer = vscode.window.tabGroups.onDidChangeTabs((event) => events.push({
    closed: event.closed.map(tabState),
    groups: vscode.window.tabGroups.all.map((group) => group.tabs.map(tabState)),
    records: [...store.drafts.values()].filter((record) => record.document && record.tabs).map(ownership),
  }));
  const persist = () => fs.writeFileSync(`${evidence}/real-file-native-trace.json`, JSON.stringify({
    cases, events, requests, notes: draftNotes, checkpoints,
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
    capture(`real-file-native-${sequence}-committed.json`);
    persist();
  };
  const closeDialog = async (record, action) => {
    const closing = vscode.commands.executeCommand('magit.closeEdit');
    await waitFor(dialog, `actual native ${action} dialog`);
    assert(record.document.isDirty);
    capture(`real-file-native-${sequence}-${action}-dialog.json`);
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
    capture('real-file-native-save-alone.json');
    await vscode.window.showTextDocument(status.document, { preview: false });
    assert.equal(head(), before, 'focus/switch never commits');
    await vscode.window.showTextDocument(record.uri, { preview: false });
    const document = await vscode.languages.setTextDocumentLanguage(record.document, 'plaintext');
    await vscode.window.showTextDocument(document, { preview: false });
    assert.equal(head(), before, 'language-change document close never commits');
    assert.equal(confirmations(record).length, 0);
    await vscode.commands.executeCommand('workbench.action.splitEditorRight');
    await waitFor(() => tabs(record).length === 2, 'same draft in two real groups');
    capture('real-file-native-two-groups.json');
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await waitFor(() => tabs(record).length === 1, 'nonlast tab closed');
    assert.equal(head(), before, 'nonlast close never commits');
    assert.equal(confirmations(record).length, 0);
    capture('real-file-native-nonlast-close.json');
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
    capture('real-file-native-cancel-retained.json');
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
    assert.match(wrongVersion.error, /exact synchronized document version/);
    assert.equal(head(), before, 'wrong-version protocol probe cannot commit');
    assert.equal(index(), oldIndex);
    assert.equal(worktree(), oldWorktree);
    assert.equal(fs.readFileSync(record.uri.fsPath, 'utf8'), saved, 'probe cannot overwrite saved bytes');
    assert.equal(record.document.getText(), liveText);
    assert(store.drafts.has(record.runId));
    cases.push({ name: 'wrong-version-protocol-probe', runId: record.runId,
      version: record.version, result: wrongVersion, head: head(), index: index(),
      worktree: worktree(), saved: fs.readFileSync(record.uri.fsPath, 'utf8'), liveText });
    capture(`real-file-native-${sequence}-wrong-version-retained.json`);
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
    capture(`real-file-native-${sequence}-unsafe-rejected.json`);
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
    capture(`real-file-native-${sequence}-unsafe-recovered.json`);
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
    capture(`real-file-native-${sequence}-stale-probes-retained.json`);
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await completed(record, before, 'native same-URI run survives stale protocol probes');

    // Two real sessions in one native host: open the second root through the
    // production view adapter and keep both provider-file runs alive together.
    const secondRepo = `${evidence}/real-file-second-repo`;
    fs.mkdirSync(secondRepo);
    execFileSync('git', ['-C', secondRepo, 'init', '-q']);
    execFileSync('git', ['-C', secondRepo, 'config', 'user.name', 'Native Evidence']);
    execFileSync('git', ['-C', secondRepo, 'config', 'user.email', 'native@example.invalid']);
    fs.writeFileSync(`${secondRepo}/tracked.txt`, 'second repository baseline\n');
    execFileSync('git', ['-C', secondRepo, 'add', 'tracked.txt']);
    execFileSync('git', ['-C', secondRepo, 'commit', '-qm', 'second baseline']);
    // A clean Create is refused before Git starts an editor helper. Each root
    // needs its own staged delta, just like start() above.
    fs.writeFileSync(`${secondRepo}/tracked.txt`, 'second repository staged delta\n');
    execFileSync('git', ['-C', secondRepo, 'add', 'tracked.txt']);
    fs.writeFileSync(`${repo}/tracked.txt`, 'first repository isolated staged delta\n');
    git('add', 'tracked.txt');
    const descriptor = await store.client.sendRequest('view/open', {
      kind: 'status', args: { repo_path: secondRepo }, viewport: { cols: 100, rows: 30 },
    });
    const secondView = views.ensureView(descriptor);
    secondView.rootId = secondView.id;
    secondView.repo = secondRepo;
    await views.showView(descriptor);
    await waitFor(() => vscode.window.activeTextEditor?.document.uri.toString() === secondView.uri.toString(),
      'second repository native status');
    await require(`${extension.extensionPath}/lib/requests`).invoke(secondView.id,
      'commit.create', undefined, { cursor: { line: 0, column: 0 } });
    await waitFor(() => [...store.drafts.values()].some((draft) => draft.attached
      && draft.uri.fsPath.startsWith(`${secondRepo}/`)), 'second repository draft');
    const draftB = [...store.drafts.values()].find((draft) => draft.uri.fsPath.startsWith(`${secondRepo}/`));
    assert(draftB);
    assert.equal(draftB.baseViewId, secondView.id, 'second session owns B');
    await vscode.window.showTextDocument(status.document, { preview: false });
    await require(`${extension.extensionPath}/lib/requests`).invoke(status.document.uri.authority,
      'commit.create', undefined, { cursor: { line: 0, column: 0 } });
    await waitFor(() => [...store.drafts.values()].some((draft) => draft.attached
      && !draft.uri.fsPath.startsWith(`${secondRepo}/`)), 'first repository draft alongside second');
    const draftA = [...store.drafts.values()].find((draft) => !draft.uri.fsPath.startsWith(`${secondRepo}/`));
    assert.equal(draftA.baseViewId, status.document.uri.authority, 'first session owns A');
    assert.notEqual(draftA.baseViewId, draftB.baseViewId);
    assert.equal(fs.realpathSync(repo), repo);
    assert.equal(fs.realpathSync(secondRepo), secondRepo);
    const repositoryState = (root) => {
      const raw = (...args) => execFileSync('git', ['-C', root, ...args], { encoding: 'utf8' });
      return { head: raw('rev-parse', 'HEAD'), tree: raw('rev-parse', 'HEAD^{tree}'),
        indexTree: raw('write-tree'), index: raw('ls-files', '--stage', '-z'),
        indexText: raw('show', ':tracked.txt'), worktree: fs.readFileSync(`${root}/tracked.txt`, 'utf8'),
        count: raw('rev-list', '--count', 'HEAD'), message: raw('log', '-1', '--format=%B'),
        status: raw('status', '--porcelain=v1', '-z') };
    };
    const snapshot = () => ({ A: repositoryState(repo), B: repositoryState(secondRepo) });
    const initial = snapshot();
    const textA = 'isolated repository A message\n';
    const textB = 'isolated repository B message\n';
    await replace(draftA, textA);
    await replace(draftB, textB);
    await draftA.document.save(); await draftA.queue;
    await draftB.document.save(); await draftB.queue;
    assert.notEqual(draftA.runId, draftB.runId);
    assert.notEqual(draftA.uri.toString(), draftB.uri.toString());
    assert.equal(store.drafts.get(draftA.runId), draftA);
    assert.equal(store.drafts.get(draftB.runId), draftB);
    const beforeProbes = snapshot();
    const pendingState = () => [draftA, draftB].map((draft) => ({ runId: draft.runId,
      owner: draft.baseViewId, pending: store.drafts.get(draft.runId) === draft,
      version: draft.version, savedVersion: draft.savedVersion, text: draft.document.getText(),
      disk: fs.readFileSync(draft.uri.fsPath, 'utf8'), closed: draft.document.isClosed,
      tabs: tabs(draft).map(tabState) }));
    const beforePending = pendingState();
    const probes = [];
    const confirmProbe = async (client, label, run, uri, source) => {
      const params = { command: 'magit.command', arguments: [{ command: 'common.confirm',
        runId: run.runId, uri: uri.uri.toString(), version: run.version, source }] };
      const result = await client.sendRequest('workspace/executeCommand', params)
        .catch((error) => ({ error: String(error) }));
      probes.push({ label, method: 'workspace/executeCommand', params, result,
        after: snapshot(), pending: pendingState() });
      const rejection = result?.error || '';
      if (label.startsWith('foreign connection') && source === 'synchronized') {
        assert.match(rejection, /exact synchronized document version/,
          'unattached foreign connection must reject a synchronized confirmation before owner lookup');
      } else {
        assert.match(rejection, /Commit draft .* (does not match|not pending)/);
      }
      assert.deepEqual(snapshot(), beforeProbes, `${label} changes neither root`);
      assert.deepEqual(pendingState(), beforePending, `${label} retains both editable runs`);
    };
    for (const source of ['synchronized', 'saved-file']) {
      await confirmProbe(store.client, 'cross-root A run with B URI', draftA, draftB, source);
      await confirmProbe(store.client, 'cross-root B run with A URI', draftB, draftA, source);
    }
    // Like editor_bridge.rs, use an independent stdio connection solely for
    // semantic owner probes. A second workbench LanguageClient would collide
    // with the live host's registered magit.command; do not deactivate it.
    const { spawn } = require('node:child_process');
    const { once } = require('node:events');
    const { createMessageConnection, StreamMessageReader, StreamMessageWriter } = require('vscode-jsonrpc/node');
    const helper = spawn(vscode.workspace.getConfiguration('magit-lsp').get('serverPath'), [],
      { stdio: ['pipe', 'pipe', 'pipe'] });
    const exited = once(helper, 'exit');
    const foreign = createMessageConnection(new StreamMessageReader(helper.stdout),
      new StreamMessageWriter(helper.stdin));
    foreign.listen();
    try {
      await foreign.sendRequest('initialize', { processId: process.pid, rootUri: null,
        capabilities: {}, clientInfo: { name: 'R-EDIT-REAL semantic owner probe' } });
      await foreign.sendNotification('initialized', {});
      for (const source of ['synchronized', 'saved-file']) {
        await confirmProbe(foreign, 'foreign connection with A run/URI', draftA, draftA, source);
        await confirmProbe(foreign, 'foreign connection with B run/URI', draftB, draftB, source);
      }
      await foreign.sendRequest('shutdown');
      await foreign.sendNotification('exit');
      const [code] = await exited;
      assert.equal(code, 0, 'independent semantic probe server exits cleanly');
    } finally {
      if (helper.exitCode === null && helper.signalCode === null) {
        helper.kill();
        await exited;
      }
      foreign.dispose();
    }
    cases.push({ name: 'two-repository-cross-wire-semantic-protocol-probes', roots: { A: repo, B: secondRepo },
      runs: { A: draftA.runId, B: draftB.runId }, uris: { A: draftA.uri.toString(), B: draftB.uri.toString() },
      owners: { A: draftA.baseViewId, B: draftB.baseViewId },
      messages: { A: textA, B: textB }, initial,
      before: beforeProbes, beforePending, probes, after: snapshot(), afterPending: pendingState() });
    capture('real-file-native-two-repository-pending.json');
    persist();
    const probeConfirmations = { A: confirmations(draftA).length, B: confirmations(draftB).length };
    await vscode.window.showTextDocument(draftA.uri, { preview: false });
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await waitFor(() => !store.drafts.has(draftA.runId) && store.drafts.has(draftB.runId),
      'native saved close only finishes repository A');
    assert.equal(message(), textA.trim());
    assert.equal(git('rev-list', '--count', `${initial.A.head.trim()}..HEAD`), '1');
    assert.equal(confirmations(draftA).length, probeConfirmations.A + 1);
    assert.equal(confirmations(draftB).length, probeConfirmations.B);
    assert.equal(repositoryState(repo).tree, initial.A.indexTree);
    assert.equal(repositoryState(repo).indexTree, initial.A.indexTree);
    assert.equal(repositoryState(repo).worktree, initial.A.worktree);
    assert.deepEqual(repositoryState(secondRepo), initial.B);
    assert.equal(draftB.document.getText(), textB);
    assert.equal(draftB.document.isClosed, false);
    assert.equal(fs.readFileSync(draftB.uri.fsPath, 'utf8'), textB);
    cases.push({ name: 'two-repository-native-A-last-close-B-pending', before: beforeProbes,
      after: snapshot(), pending: pendingState(), confirmations: confirmations(draftA) });
    await vscode.window.showTextDocument(draftB.uri, { preview: false });
    capture('real-file-native-two-repository-A-committed-B-pending.json');
    const beforeAbort = snapshot();
    await vscode.commands.executeCommand('magit.cancelEdit');
    await waitFor(() => !store.drafts.has(draftB.runId), 'explicit abort of repository B');
    assert.deepEqual(snapshot(), beforeAbort, 'explicit native Abort preserves both exact repository states');
    assert.equal(fs.readFileSync(draftB.uri.fsPath, 'utf8'), textB);
    cases.push({ name: 'two-repository-native-A-close-abort-B', beforeAbort,
      after: snapshot(), bFile: fs.readFileSync(draftB.uri.fsPath, 'utf8') });
    capture('real-file-native-two-repository-A-committed-B-aborted.json');
    await vscode.window.showTextDocument(draftB.uri, { preview: false });
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    assert.equal(tabs(draftB).length, 0);
    assert.deepEqual(snapshot(), beforeAbort, 'late native close after abort cannot commit B');
    assert.equal(fs.readFileSync(draftB.uri.fsPath, 'utf8'), textB);
    assert.equal(confirmations(draftB).length, probeConfirmations.B);
    assert.equal(store.drafts.size, 0);
    cases.push({ name: 'two-repository-late-native-B-close-after-abort', before: beforeAbort,
      after: snapshot(), bFile: fs.readFileSync(draftB.uri.fsPath, 'utf8'),
      confirmations: confirmations(draftB) });
    capture('real-file-native-two-repository-late-B-close.json');
    persist();
    fs.writeFileSync(`${evidence}/real-file-native-evidence.json`, JSON.stringify({
      gate: 'R-EDIT-REAL production native close', status: 'pass', cases, checkpoints,
      trace: 'real-file-native-trace.json',
    }, null, 2));
  } finally {
    observer.dispose();
    persist();
  }
};
