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
