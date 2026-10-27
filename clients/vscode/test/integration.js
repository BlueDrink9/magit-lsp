'use strict';

/**
 * Node integration test: activate the real extension against the real server.
 *
 * A scripted `vscode` module supplies the editor widgets and virtual
 * documents; the fake `LanguageClient` speaks LSP to `target/debug/magit-lsp`
 * over stdio. The test proves the thin-client contract end to end: open a view,
 * render the server's text verbatim, press a key that triggers a render, use a
 * menu QuickPick, and answer a server prompt.
 */

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const Module = require('node:module');

const vscode = require('./fake/vscode');
const { LanguageClient, instances } = require('./fake/languageclient');

/** Test root: `clients/vscode/test`. */
const here = __dirname;

/**
 * Resolve after `milliseconds`; used only between observation polls.
 *
 * @param {number} milliseconds
 * @returns {Promise<void>}
 */
function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/**
 * Poll `condition` until it is truthy or the timeout expires.
 *
 * @param {() => unknown} condition
 * @param {string} label
 * @param {number} [timeout]
 * @returns {Promise<void>}
 */
async function waitFor(condition, label, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (!condition()) {
    if (Date.now() > deadline) {
      throw new Error(`timed out waiting for ${label}`);
    }
    await delay(20);
  }
}

/**
 * Run Git in the fixture repository.
 *
 * @param {string} repo
 * @param {string[]} args
 */
function git(repo, args) {
  execFileSync('git', ['-C', repo, ...args], { stdio: 'ignore' });
}

/**
 * Create a repository with one modified tracked file.
 *
 * @returns {string} repository path
 */
function createRepository() {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-lsp-vscode-'));
  git(repo, ['init', '-q']);
  git(repo, ['config', 'user.name', 'Test']);
  git(repo, ['config', 'user.email', 'test@example.com']);
  fs.writeFileSync(path.join(repo, 'tracked.txt'), 'original\n');
  git(repo, ['add', '.']);
  git(repo, ['commit', '-qm', 'initial']);
  fs.writeFileSync(path.join(repo, 'tracked.txt'), 'changed\n');
  return repo;
}

/**
 * Load `extension.js` with the fake editor modules installed.
 *
 * @returns {object} the extension module
 */
function loadExtension() {
  const originalLoad = Module._load;
  Module._load = function load(request, parent, isMain) {
    if (request === 'vscode') return vscode;
    if (request === 'vscode-languageclient/node') return { LanguageClient };
    return originalLoad.call(this, request, parent, isMain);
  };
  try {
    return require('../extension');
  } finally {
    Module._load = originalLoad;
  }
}

/** Run the scenario, reporting product output when an assertion fails. */
async function run() {
  const binary = process.env.MAGIT_LSP_SERVER_PATH
    || path.resolve(here, '../../../target/debug/magit-lsp');
  if (!fs.existsSync(binary)) {
    throw new Error(`missing ${binary}; run \`cargo build\` or \`make build\` first`);
  }

  const repo = createRepository();
  vscode.__test.reset();
  vscode.__test.setWorkspaceFolder(repo);
  vscode.__test.setConfiguration({
    'magit-lsp.serverPath': binary,
    'magit-lsp.profile': 'standard',
  });

  const extension = loadExtension();
  const context = { subscriptions: [] };
  extension.activate(context);

  try {
    const commands = vscode.__test.commands;
    const client = instances.at(-1);
    assert(client, 'the extension started a language client');

    // 1. Open the status view and display the server's text verbatim.
    await commands.get('magit.open')();
    await waitFor(
      () => vscode.__test.activeEditor
        && /Unstaged changes/.test(vscode.__test.activeEditor.document.getText()),
      'status view text'
    );
    const statusEditor = vscode.__test.activeEditor;
    const openedText = statusEditor.document.getText();
    assert.match(openedText, /Unstaged changes/);
    assert.equal(statusEditor.document.uri.scheme, 'magit');
    assert(
      client.requests.some((request) => request.method === 'view/open'),
      'opened the view through view/open'
    );
    assert(
      client.notes.some((note) => note.method === 'view/rendered'),
      'acknowledged the first render'
    );

    // 2. A key press goes to the server and its render replaces the text.
    fs.writeFileSync(path.join(repo, 'added.txt'), 'new\n');
    const rendersBefore = client.notes.filter((note) => note.method === 'view/rendered').length;
    await commands.get('magit.key')({ key: 'g' });
    await waitFor(
      () => /added\.txt/.test(statusEditor.document.getText()),
      'refreshed status text'
    );
    assert(
      client.requests.some((request) => request.method === 'view/key'
        && request.params.key === 'g'
        && request.params.viewId === 'v1'),
      'forwarded the canonical key with the server view id'
    );
    assert(
      client.notes.filter((note) => note.method === 'view/rendered').length > rendersBefore,
      'acknowledged the render caused by the key press'
    );

    await require('./selection-integration').checkShapes({ vscode, commands, client, editor: statusEditor });

    // Static mode is a metadata-only transition: it must not invalidate the
    // virtual document or disturb the user's selection.
    const statusState = require('../lib/store').store.views.get('v1');
    const selection = new vscode.Selection(new vscode.Position(0, 0), new vscode.Position(0, 3));
    statusEditor.selection = selection;
    const linesBeforeStatic = statusState.lines;
    const versionBeforeStatic = statusState.version;
    let documentsChangedCount = 0;
    const documentsChangedSubscription = require('../lib/store').store.documentsChanged.event(() => {
      documentsChangedCount += 1;
    });
    const changedBeforeStatic = client.notes.filter((note) => note.method === 'view/rendered').length;
    await commands.get('magit.invoke')({ command: 'common.toggle-text-mode' });
    await waitFor(() => vscode.__test.contextValues.get('magit.textMode') === true,
      'text-mode context');
    assert.equal(vscode.__test.contextValues.get('magit.textMode'), true);
    assert.equal(statusEditor.selection, selection);
    assert.equal(statusState.lines, linesBeforeStatic);
    assert.equal(statusState.version, versionBeforeStatic);
    assert.equal(documentsChangedCount, 0);
    assert.equal(client.notes.filter((note) => note.method === 'view/rendered').length, changedBeforeStatic + 1);
    assert.equal(client.notes.at(-1).method, 'view/rendered');
    assert.equal(client.notes.at(-1).params.version, versionBeforeStatic);

    // Refreshes queued while static are applied as a full server render on
    // return to interactive mode.
    fs.writeFileSync(path.join(repo, 'queued.txt'), 'queued\n');
    await commands.get('magit.key')({ key: 'g' });
    assert.equal(statusState.lines, linesBeforeStatic);
    assert.equal(statusEditor.selection, selection);
    await commands.get('magit.invoke')({ command: 'common.toggle-text-mode' });
    await waitFor(() => vscode.__test.contextValues.get('magit.textMode') === false,
      'interactive-mode context');
    await waitFor(() => /queued\.txt/.test(statusEditor.document.getText()), 'queued refresh after static mode');
    assert.equal(vscode.__test.contextValues.get('magit.textMode'), false);
    assert.notEqual(statusState.lines, linesBeforeStatic);
    documentsChangedSubscription.dispose();

    // 3. A server menu becomes a QuickPick; selecting a row sends its key.
    let promptItems;
    vscode.__test.setQuickPickAnswer((items) => {
      promptItems = items;
      return items.find((item) => item.id === 'no');
    });
    await commands.get('magit.invoke')({ command: 'open.commit' });
    await waitFor(
      () => vscode.__test.lastQuickPick
        && vscode.__test.lastQuickPick.items.some((item) => item.key === 'c'),
      'commit menu QuickPick'
    );
    const menu = vscode.__test.lastQuickPick;
    const initialMenuState = require('../lib/store').store.views.get(
      require('../lib/store').store.quickPickViewId
    );
    assert.match(menu.title, /Commit/);
    menu.select(menu.items.find((item) => item.key === 'c'));

    // 4. The menu action prompts, and the scripted answer reaches the server.
    await waitFor(
      () => client.requests.some((request) => request.method === 'view/promptRespond'),
      'prompt response'
    );
    const answer = client.requests.find(
      (request) => request.method === 'view/promptRespond'
    ).params;
    assert.equal(answer.cancelled, false);
    assert.equal(answer.value, false, 'answered the confirm prompt with No');
    assert(promptItems, 'the prompt offered choices through showQuickPick');
    assert.deepEqual(promptItems.map((item) => item.id), ['yes', 'no']);
    assert(
      client.requests.some((request) => request.method === 'view/key'
        && request.params.key === 'c'),
      'the menu row was forwarded as a canonical key, not a client command'
    );

    // Exercise real showMenu accept handlers on live menu views. Synthetic
    // structured rows cover canonical shapes absent from the standard profile;
    // their keys still travel through requests.press to the real server.
    const { updateQuickPick } = require('../lib/menus');
    const viewStore = require('../lib/store').store;
    await initialMenuState.pressQueue;
    const keyRequestsSince = (start, id) => client.requests.slice(start)
      .filter((request) => request.method === 'view/key' && request.params.viewId === id)
      .map((request) => request.params.key);
    const activationCases = [
      ['-a', ['-', 'a']], ['=A', ['=', 'A']], ['g l', ['g', 'l']],
      ['ctrl+t', ['ctrl+t']], ['Enter', ['Enter']], ['G', ['G']], ['Plus', ['Plus']],
    ];
    for (const [key, activationSteps] of activationCases) {
      await commands.get('magit.invoke')({ command: 'open.commit' });
      await waitFor(() => viewStore.views.get(viewStore.quickPickViewId)?.quickPick,
        `live menu for ${key}`);
      const state = viewStore.views.get(viewStore.quickPickViewId);
      assert(state?.quickPick, `live showMenu widget for ${key}`);
      state.menu = { ...state.menu, groups: [{ title: 'Activation regression', items: [{
        kind: 'action', key, activationSteps, label: 'Planned activation',
        status: { status: 'planned', roadmapId: 'R-4.1' },
      }] }] };
      updateQuickPick(state);
      const row = state.quickPick.items.find((item) => item.key === key);
      assert.equal(row.status, 'planned');
      assert.deepEqual(row.activationSteps, activationSteps);
      const start = client.requests.length;
      state.quickPick.select(row);
      await state.pressQueue;
      assert.deepEqual(keyRequestsSince(start, state.id), activationSteps, key);
      state.quickPick.value = 'q';
      await state.pressQueue;
      assert.equal(viewStore.views.has(state.id), false, 'real server close applied');
    }

    await commands.get('magit.invoke')({ command: 'open.commit' });
    await waitFor(() => viewStore.views.get(viewStore.quickPickViewId)?.quickPick,
      'live typed-input menu');
    const typedState = viewStore.views.get(viewStore.quickPickViewId);
    const typedStart = client.requests.length;
    typedState.quickPick.value = 'g l+G';
    await typedState.pressQueue;
    assert.deepEqual(keyRequestsSince(typedStart, typedState.id), ['g', 'Space', 'l', 'Plus', 'G']);
    assert.equal(typedState.quickPick.value, '', 'typed input is still cleared');

    // Acceptance queues stop after an actual server close, including later
    // accept events queued before that close has arrived.
    typedState.menu = { ...typedState.menu, groups: [{ title: 'Close regression', items: [{
      kind: 'action', key: 'q x', activationSteps: ['q', 'x'], label: 'Close then tail',
      status: { status: 'implemented' },
    }] }] };
    updateQuickPick(typedState);
    const closeStart = client.requests.length;
    const closingPick = typedState.quickPick;
    closingPick.select(closingPick.items.find((item) => item.key === 'q x'));
    closingPick.select(closingPick.items.find((item) => item.key === 'q x'));
    await typedState.pressQueue;
    assert.deepEqual(keyRequestsSince(closeStart, typedState.id), ['q']);
    assert.equal(viewStore.views.has(typedState.id), false);

    // The real prompt handler must interpret the first displayed row as No
    // when the server's default is false (including destructive prompts).
    const { promptFor } = require('../lib/requests');
    vscode.__test.setQuickPickAnswer((items) => items[0]);
    for (const vector of require('../../../tests/data/client-contracts.json').prompts
      .filter((item) => item.name === 'confirm-default-false'
        || item.name === 'confirm-destructive')) {
      const result = await promptFor(vector.params);
      assert.deepEqual(result, vector.answer, vector.name);
    }

    // Exercise the actual editor bridge through the registered dispatcher and
    // generated binding payloads rather than simulating profile key chords.
    const invokeGenerated = async (command) => {
      const binding = require('../package.json').contributes.keybindings.find((entry) =>
        entry.args?.command === command
          && entry.when.includes("magit.scope == 'commit-message'"));
      assert(binding, `generated binding for ${command}`);
      await commands.get(binding.command)(binding.args);
    };
    git(repo, ['add', 'tracked.txt']);
    await commands.get('magit.invoke')({ command: 'commit.create' });
    await waitFor(() => vscode.__test.activeEditor?.document.uri.scheme === 'file'
      && vscode.__test.activeEditor.document.languageId === 'git-commit', 'bridge edit document');
    const editDocument = vscode.__test.activeEditor.document;
    const commitMessage = `integration bridge ${Date.now()}`;
    fs.writeFileSync(editDocument.uri.fsPath, `${commitMessage}\n`);
    await invokeGenerated('common.confirm');
    await waitFor(() => execFileSync('git', ['-C', repo, 'log', '-1', '--format=%B'])
      .toString().trim() === commitMessage, 'confirmed bridge commit');

    fs.writeFileSync(path.join(repo, 'tracked.txt'), `cancel probe ${Date.now()}\n`);
    git(repo, ['add', 'tracked.txt']);
    await commands.get('magit.invoke')({ command: 'commit.create' });
    await waitFor(() => vscode.__test.activeEditor?.document.uri.scheme === 'file'
      && vscode.__test.activeEditor.document.languageId === 'git-commit', 'second bridge edit');
    await invokeGenerated('common.cancel');
    await waitFor(() => require('../lib/store').store.edits.size === 0,
      'cancelled bridge edit is removed');
    assert.equal(execFileSync('git', ['-C', repo, 'log', '-1', '--format=%B'])
      .toString().trim(), commitMessage, 'cancelled bridge edit did not commit');

    // The displayed text is always the server's last render; never recomposed.
    assert.match(statusEditor.document.getText(), /Unstaged changes/);
    assert.equal(vscode.__test.output.filter((line) => line.startsWith('unknown effect')).length, 0);

    // Quit is dispatched through the registered command and scoped to the
    // repository that owns the originating view. This harness uses mocked
    // VSCode APIs; it is not evidence of native tab behavior.
    const { store } = require('../lib/store');
    const secondRepo = createRepository();
    vscode.__test.setWorkspaceFolder(secondRepo);
    await commands.get('magit.open')();
    await waitFor(() => store.views.size === 2, 'second repository view');
    await commands.get('magit.invoke')({ command: 'common.quit' });
    assert.equal(store.views.size, 1, 'quit removes only the originating repository view');
    assert.equal(Array.from(store.views.values())[0].repo, repo);
    vscode.__test.setWorkspaceFolder(repo);
    await commands.get('magit.open')();
    await commands.get('magit.invoke')({ command: 'common.quit' });
    assert.equal(store.views.size, 0, 'root quit removes the affected stored view');
    await commands.get('magit.open')();
    await waitFor(() => store.views.size === 1, 'reopened repository view');
    fs.rmSync(secondRepo, { recursive: true, force: true });
    await require('./selection-integration').checkGitSelections({ vscode, commands, client });
  } finally {
    await extension.deactivate();
  }

  const exited = await Promise.race([
    instances.at(-1).exited.then(() => true),
    delay(5000).then(() => false),
  ]);
  assert(exited, 'the language server exited after deactivate');
  fs.rmSync(repo, { recursive: true, force: true });
}

run().then(() => {
  console.log('ok node integration');
}).catch((error) => {
  console.error(error && error.stack ? error.stack : error);
  for (const client of instances) {
    if (client.stderr) console.error(`--- magit-lsp stderr ---\n${client.stderr}`);
  }
  for (const message of vscode.__test.messages) {
    console.error(`--- ${message.level}: ${message.text}`);
  }
  for (const line of vscode.__test.output) {
    console.error(`--- output: ${line}`);
  }
  process.exitCode = 1;
});
