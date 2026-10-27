'use strict';

const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');

const vscode = require('vscode');

const edits = require('./edits');
const foldkeys = require('./foldkeys');
const menus = require('./menus');
const requests = require('./requests');
const views = require('./views');
const { SCHEME, store } = require('./store');
const { message } = require('./util');

/**
 * The editor mode, cursor and selection a key press reports.
 *
 * @returns {{cursor: {line: number, column: number}, selection?: {startLine: number, endLine: number}}}
 */
function editorInfo() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.uri.scheme !== SCHEME) {
    return { cursor: { line: 0, column: 0 } };
  }

  const selection = editor.selection;
  const cursor = { line: selection.active.line, column: selection.active.character };
  if (selection.isEmpty) {
    return { cursor };
  }
  return {
    cursor,
    selection: {
      startLine: selection.start.line,
      endLine: selection.end.line > selection.start.line && selection.end.character === 0
        ? selection.end.line - 1
        : selection.end.line,
    },
  };
}

/**
 * Open the status view for the first workspace folder.
 */
async function openStatus() {
  const client = store.client;
  if (!client) {
    return;
  }
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    vscode.window.showErrorMessage('magit-lsp: open a repository folder first.');
    return;
  }

  try {
    await store.clientReady;
    const repo = folders[0].uri.fsPath;
    if (store.statusOpens.has(repo)) {
      await store.statusOpens.get(repo);
    }
    const existing = Array.from(store.views.values()).find(
      (state) => state.repo === repo && state.descriptor.kind === 'status'
    );
    if (existing) {
      await views.focusView(existing.id);
      return;
    }
    const opening = (async () => {
      const descriptor = await client.sendRequest('view/open', {
      kind: 'status',
      args: { repo_path: repo },
      viewport: viewport(),
      });
      const state = views.ensureView(descriptor);
      state.repo = repo;
      state.rootId = state.id;
      await views.showView(descriptor);
    })();
    store.statusOpens.set(repo, opening);
    try {
      await opening;
    } finally {
      store.statusOpens.delete(repo);
    }
  } catch (error) {
    vscode.window.showErrorMessage(`magit-lsp: ${message(error)}`);
  }
}

/**
 * The viewport used for the first render. Menus are native QuickPicks, so this
 * only affects text views.
 *
 * @returns {{cols: number, rows: number}}
 */
function viewport() {
  return { cols: 100, rows: 30 };
}

/**
 * `magit.key`: forward one canonical key step.
 *
 * @param {{key?: string}|undefined} args
 */
async function pressCommand(args) {
  const key = args && args.key;
  if (!key) {
    return;
  }

  const state = views.activeState();
  if (!state) {
    return;
  }

  if (state.quickPick || views.isMenuKind(state.descriptor.kind)) {
    menus.queueSteps(state, [key]);
    return;
  }

  state.pressQueue = state.pressQueue
    .then(() => requests.press(state.id, key, editorInfo()))
    .catch((error) => {
      store.output.appendLine(`key failed: ${message(error)}`);
    });
  await state.pressQueue;
}

/**
 * `magit.invoke`: run a command by ID.
 *
 * @param {{command?: string, args?: unknown}|string|undefined} args
 */
async function invokeCommand(args) {
  const command = typeof args === 'string' ? args : args && args.command;
  if (!command) {
    return;
  }
  // The focused edit document has no Magit view; generated command IDs still
  // identify the server-owned edit actions.
  const currentEdit = edits.currentEdit();
  if (currentEdit && currentEdit.effect.scope) {
    if (command === 'common.confirm') await edits.confirmEditCommand();
    else if (command === 'common.cancel') await edits.cancelEditCommand();
    return;
  }
  const state = views.activeState();
  if (!state) {
    return;
  }

  const info = state.quickPick ? { cursor: { line: 0, column: 0 } } : editorInfo();
  state.pressQueue = state.pressQueue
    .then(() => requests.invoke(
      state.id,
      command,
      typeof args === 'object' ? args.args : undefined,
      info
    ))
    .catch((error) => {
      store.output.appendLine(`invoke failed: ${message(error)}`);
    });
  await state.pressQueue;
}

/**
 * `magit.mirrorFoldShortcuts`: show the user's fold keys as `magit.invoke`
 * entries that can be pasted into `keybindings.json`.
 */
async function mirrorFoldShortcuts() {
  const file = keybindingsPath();
  let text;
  try {
    text = await fs.readFile(file, 'utf8');
  } catch (error) {
    vscode.window.showInformationMessage(`magit-lsp: no keybindings.json at ${file}`);
    return;
  }

  let entries;
  try {
    entries = foldkeys.mapFoldKeys(text);
  } catch (error) {
    vscode.window.showErrorMessage(`magit-lsp: cannot read ${file}: ${message(error)}`);
    return;
  }

  if (entries.length === 0) {
    vscode.window.showInformationMessage('magit-lsp: no editor fold shortcuts found.');
    return;
  }

  const content = `${JSON.stringify(entries, null, 2)}\n`;
  const document = await vscode.workspace.openTextDocument({ language: 'jsonc', content });
  await vscode.window.showTextDocument(document, { preview: true });

  const choice = await vscode.window.showInformationMessage(
    `magit-lsp: ${entries.length} fold shortcut(s) can be mirrored.`,
    'Copy to clipboard'
  );
  if (choice === 'Copy to clipboard') {
    await vscode.env.clipboard.writeText(content);
  }
}

/**
 * The user's VSCode `keybindings.json`.
 *
 * @returns {string}
 */
function keybindingsPath() {
  const configured = vscode.workspace.getConfiguration('magit-lsp').get('keybindingsPath');
  if (configured) {
    return configured;
  }

  const home = os.homedir();
  if (process.platform === 'darwin') {
    return path.join(home, 'Library', 'Application Support', 'Code', 'User', 'keybindings.json');
  }
  if (process.platform === 'win32') {
    return path.join(
      process.env.APPDATA || path.join(home, 'AppData', 'Roaming'),
      'Code',
      'User',
      'keybindings.json'
    );
  }
  return path.join(
    process.env.XDG_CONFIG_HOME || path.join(home, '.config'),
    'Code',
    'User',
    'keybindings.json'
  );
}

exports.openStatus = openStatus;
exports.viewport = viewport;
exports.pressCommand = pressCommand;
exports.invokeCommand = invokeCommand;
exports.mirrorFoldShortcuts = mirrorFoldShortcuts;
exports.keybindingsPath = keybindingsPath;
