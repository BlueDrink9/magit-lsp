'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const vscode = require('vscode');

/** Exercise commit HEAD operations through the production commit menu. */
exports.run = async function run({ extension, evidence, repo, cdp, waitFor, shot,
  requests, draftMessages, draftNotes }) {
  const { store } = require(`${extension.extensionPath}/lib/store`);
  const raw = (...args) => execFileSync('git', ['-C', repo, ...args], { encoding: 'utf8' });
  const git = (...args) => raw(...args).trim();
  const status = vscode.window.activeTextEditor;
  assert.equal(status?.document.uri.scheme, 'magit');
  const cases = [];
  const screenshots = [];
  const openedDrafts = [];
  const observer = vscode.workspace.onDidOpenTextDocument((document) => {
    if (document.uri.fsPath.endsWith('/COMMIT_EDITMSG')) openedDrafts.push(document.uri.toString());
  });
  const snapshot = () => ({ head: git('rev-parse', 'HEAD'), tree: git('rev-parse', 'HEAD^{tree}'),
    index: git('write-tree'), indexEntries: raw('ls-files', '--stage'),
    parents: git('show', '-s', '--format=%P', 'HEAD'), count: git('rev-list', '--count', 'HEAD'),
    date: git('show', '-s', '--format=%cI', 'HEAD'), message: raw('log', '-1', '--format=%B'),
    worktree: git('hash-object', 'tracked.txt'), porcelain: raw('status', '--porcelain'),
    untracked: raw('ls-files', '--others', '--exclude-standard'),
    sentinel: fs.existsSync(`${repo}/untracked.txt`) ? git('hash-object', 'untracked.txt') : null });
  const capture = async (name) => {
    const expected = vscode.window.activeTextEditor.document.getText().split('\n')
      .map((line) => line.trim()).filter(Boolean).slice(0, 3);
    await waitFor(() => {
      const painted = cdp('Runtime.evaluate', { expression:
        'Array.from(document.querySelectorAll(".monaco-editor .view-line"), line => line.textContent.replace(/\\u00a0/g, " ").trim())',
      returnByValue: true }).result.value;
      return painted.length && (store.quickPickViewId || expected.every((line) => painted.includes(line)));
    }, `${name} native editor painted`);
    screenshots.push(name);
    shot(name);
  };
  const persist = () => fs.writeFileSync(`${evidence}/head-operations-trace.json`, JSON.stringify({
    cases, screenshots, requests, draftMessages, draftNotes, openedDrafts,
  }, null, 2));
  const focusStatus = async () => {
    await vscode.window.showTextDocument(status.document, { preview: false });
    await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
    const state = store.views.get(status.document.uri.authority);
    const version = state.version;
    await vscode.commands.executeCommand('magit.invoke', { command: 'common.refresh' });
    await waitFor(() => state.version > version && status.document.getText() === state.lines.join('\n'),
      'server-owned status applied');
  };
  const menu = async () => {
    await focusStatus();
    await vscode.commands.executeCommand('magit.invoke', { command: 'open.commit' });
    let state;
    await waitFor(() => {
      state = [...store.views.values()].find((view) => view.descriptor.kind === 'menu.commit');
      return state?.quickPick && state.menu;
    }, 'production commit QuickPick');
    await waitFor(() => cdp('Runtime.evaluate', { expression:
      'Boolean(document.activeElement?.closest(".quick-input-widget"))', returnByValue: true }).result.value,
    'native QuickPick keyboard focus');
    return state;
  };
  const select = async (state, canonical, name) => {
    const row = state.menu.groups.flatMap((group) => group.items).find((item) => item.key === canonical);
    assert(row, `missing server menu row ${canonical}`);
    await waitFor(() => state.quickPick.activeItems[0]?.key, 'native initial menu selection');
    await capture(`head-${name}-menu.png`);
    const start = requests.length;
    // Type into the actual native QuickPick. Its production onDidChangeValue
    // routes each captured key; never invoke an action or set selectedItems.
    for (const [index, step] of row.activationSteps.entries()) {
      cdp('Input.insertText', { text: step });
      await waitFor(() => requests.slice(start).filter((request) => request.method === 'view/key').length > index,
        `native typed menu step ${step}`);
      await state.pressQueue;
    }
    await waitFor(() => requests.slice(start).some((request) => request.method === 'view/key'), 'native row acceptance');
    await state.pressQueue;
    assert.deepEqual(requests.slice(start).filter((request) => request.method === 'view/key')
      .map((request) => request.params.key), row.activationSteps, 'server activation steps');
  };
  const action = async (selection, name, flag) => {
    const state = await menu();
    if (flag) {
      await select(state, flag, `${name}-option`);
      await waitFor(() => state.menu.groups.flatMap((group) => group.items)
        .some((item) => item.key === flag && item.on), 'captured commit option');
    }
    await select(state, selection, name);
    await waitFor(() => !store.views.has(state.id), 'commit menu popped');
  };
  const tabs = (record) => vscode.window.tabGroups.all.flatMap((group) => group.tabs)
    .filter((tab) => tab.input?.uri?.toString() === record.uri.toString());
  const draft = async (name) => {
    await waitFor(() => [...store.drafts.values()].some((record) => record.attached), 'real Git COMMIT_EDITMSG');
    const record = [...store.drafts.values()][0];
    assert.equal(record.uri.scheme, 'file');
    assert(record.uri.fsPath.endsWith('/COMMIT_EDITMSG'));
    assert.equal(tabs(record).length, 1);
    assert.equal(record.document.getText(), fs.readFileSync(record.uri.fsPath, 'utf8'));
    assert(record.document.getText().includes('Exact retained body.'));
    await capture(`head-${name}-draft.png`);
    return record;
  };
  const replace = async (record, text) => {
    const editor = await vscode.window.showTextDocument(record.uri, { preview: false });
    assert(await editor.edit((edit) => edit.replace(new vscode.Range(record.document.positionAt(0),
      record.document.positionAt(record.document.getText().length)), text)));
    await waitFor(() => record.text === text && !record.flushQueued, 'real draft full sync');
  };
  const savedClose = async (record, before, name) => {
    await record.document.save();
    await record.queue;
    assert.equal(git('rev-parse', 'HEAD'), before.head, 'save alone must not commit');
    const saved = fs.readFileSync(record.uri.fsPath, 'utf8');
    assert.equal(saved, record.document.getText());
    await capture(`head-${name}-saved.png`);
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await waitFor(() => !store.drafts.has(record.runId) && git('rev-parse', 'HEAD') !== before.head,
      'native saved last-tab close replaces HEAD');
    assert.equal(tabs(record).length, 0);
    const confirmations = requests.filter((request) => request.method === 'workspace/executeCommand'
      && request.params.arguments?.[0]?.runId === record.runId
      && request.params.arguments[0].command === 'common.confirm');
    assert.equal(confirmations.length, 1);
    assert.equal(confirmations[0].params.arguments[0].source, 'saved-file');
    return { runId: record.runId, saved, confirmation: confirmations[0] };
  };
  const replaced = (before, after, tree, message, preserveDate = false) => {
    assert.notEqual(after.head, before.head);
    assert.equal(after.parents, before.parents, 'replacement retains original parents');
    assert.equal(after.count, before.count, 'replacement is not a descendant');
    assert.equal(after.tree, tree);
    assert.equal(after.message, message);
    assert.equal(after.index, before.index);
    assert.equal(after.indexEntries, before.indexEntries);
    assert.equal(after.worktree, before.worktree);
    if (preserveDate) assert.equal(after.date, before.date);
  };
  const recordCase = async (name, before, extra = {}) => {
    await focusStatus();
    await capture(`head-${name}-status.png`);
    cases.push({ name, before, after: snapshot(), status: status.document.getText(), ...extra });
    persist();
  };
  try {
    git('config', 'user.name', 'Test');
    git('config', 'user.email', 'test@example.com');
    git('add', '.');
    execFileSync('git', ['-C', repo, 'commit', '-qm', 'Native HEAD baseline', '-m', 'Exact retained body.'],
      { env: { ...process.env, GIT_COMMITTER_DATE: '2001-01-01T00:00:00+0000' } });
    const baseline = snapshot();
    assert(baseline.parents);
    const reset = async (content, staged = true) => {
      await waitFor(() => store.drafts.size === 0, 'no previous draft');
      git('reset', '--hard', baseline.head);
      fs.rmSync(`${repo}/untracked.txt`, { force: true });
      if (content) {
        fs.writeFileSync(`${repo}/tracked.txt`, content);
        if (staged) git('add', 'tracked.txt');
      }
      await focusStatus();
    };
    for (const clean of [false, true]) {
      const name = `amend-${clean ? 'clean' : 'staged'}`;
      await reset(clean ? null : 'staged amend sentinel\n');
      const before = snapshot();
      await action('a', name);
      const record = await draft(name);
      const close = await savedClose(record, before, name);
      const after = snapshot();
      replaced(before, after, before.index, baseline.message);
      assert.equal(after.porcelain, '');
      await recordCase(name, before, close);
    }
    await reset('staged reword sentinel\n');
    let before = snapshot();
    assert.notEqual(before.index, before.tree, 'genuinely nonempty staged index');
    assert(git('diff', '--cached', '--name-only'));
    await action('w', 'reword');
    let record = await draft('reword');
    await replace(record, 'Native reworded message\n\nExact edited body.\n');
    const close = await savedClose(record, before, 'reword');
    replaced(before, snapshot(), before.tree, 'Native reworded message\n\nExact edited body.\n\n', true);
    assert.equal(snapshot().porcelain, before.porcelain);
    await recordCase('reword', before, close);

    for (const all of [false, true]) {
      const name = all ? 'extend-tracked-all' : 'extend-staged';
      await reset(`${name}\n`, !all);
      let expectedTree = git('write-tree');
      let expectedEntries = raw('ls-files', '--stage');
      if (all) {
        fs.writeFileSync(`${repo}/untracked.txt`, 'untracked sentinel\n');
        git('add', '-u');
        expectedTree = git('write-tree');
        expectedEntries = raw('ls-files', '--stage');
        git('reset', '--quiet', 'HEAD');
      }
      before = snapshot();
      const opened = openedDrafts.length;
      const notes = draftNotes.length;
      const messages = draftMessages.length;
      await action('e', name, all ? '-a' : null);
      await waitFor(() => draftMessages.slice(messages).some((message) => /Committed/.test(message.text))
        && snapshot().head !== before.head, `${name} completed`);
      assert.equal(store.drafts.size, 0);
      assert.equal(openedDrafts.length, opened, 'Extend opens no editor');
      assert.equal(draftNotes.length, notes, 'Extend sends no editor document traffic');
      const after = snapshot();
      replaced({ ...before, index: expectedTree, indexEntries: expectedEntries }, after,
        expectedTree, baseline.message, true);
      assert.equal(after.untracked, before.untracked);
      assert.equal(after.sentinel, before.sentinel);
      assert.equal(after.porcelain, all ? '?? untracked.txt\n' : '');
      if (all) assert.equal(git('ls-files', '--', 'untracked.txt'), '');
      await recordCase(name, before, { expectedTree, expectedEntries, openedDrafts: openedDrafts.length - opened });
    }
    await reset();
    before = snapshot();
    let messages = draftMessages.length;
    await action('e', 'extend-clean-guard');
    await waitFor(() => draftMessages.slice(messages).some((message) =>
      /Nothing staged \(or unstaged\) to extend/.test(message.text)), 'readable clean Extend guard');
    assert.deepEqual(snapshot(), before);
    await recordCase('extend-clean-guard', before);
    messages = draftMessages.length;
    const opened = openedDrafts.length;
    await action('e', 'extend-clean-allowed', '-e');
    await waitFor(() => draftMessages.slice(messages).some((message) => /Committed/.test(message.text)),
      'allow-empty identical Extend completed');
    assert.deepEqual(snapshot(), before, 'byte-identical Extend retains exact OID/state');
    assert.equal(openedDrafts.length, opened);
    assert.equal(store.drafts.size, 0);
    await recordCase('extend-clean-allowed', before);

    await reset('cancel staged\n');
    fs.appendFileSync(`${repo}/tracked.txt`, 'cancel unstaged\n');
    before = snapshot();
    await action('a', 'amend-abort');
    record = await draft('amend-abort');
    await replace(record, 'Explicitly cancelled amendment\n');
    await capture('head-amend-abort-edited.png');
    await vscode.commands.executeCommand('magit.cancelEdit');
    await waitFor(() => !store.drafts.has(record.runId), 'explicit abort finished');
    assert.deepEqual(snapshot(), before, 'explicit abort preserves exact HEAD/index/worktree');
    assert(!requests.some((request) => request.method === 'workspace/executeCommand'
      && request.params.arguments?.[0]?.runId === record.runId
      && request.params.arguments[0].command === 'common.confirm'));
    await recordCase('amend-abort', before, { runId: record.runId });
    fs.writeFileSync(`${evidence}/head-operations-evidence.json`, JSON.stringify({
      gate: 'R-EDIT-REAL HEAD operations', status: 'pass', baseline, cases, screenshots,
      trace: 'head-operations-trace.json',
    }, null, 2));
    console.log(`PASS native HEAD operations (${cases.length} exact repository-state cases)`);
  } finally {
    observer.dispose();
    persist();
  }
};
