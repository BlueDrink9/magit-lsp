'use strict';

const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');

const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

const contextKeys = require('./lib/context');
const render = require('./lib/render');
const menu = require('./lib/menu');
const foldkeys = require('./lib/foldkeys');
const prompt = require('./lib/prompt');

const SCHEME = 'magit';

/** @type {LanguageClient|undefined} */
let client;

/** @type {Promise<void>|undefined} */
let clientReady;

/** @type {vscode.OutputChannel|undefined} */
let output;

/** @type {vscode.EventEmitter<vscode.Uri>|undefined} */
let documentsChanged;

/** @type {string|undefined} */
let activeViewId;

/** @type {string|undefined} */
let quickPickViewId;

/** All views the server opened in this window, keyed by view id. */
const views = new Map();

/** Virtual document URI -> view id. */
const uriToView = new Map();

/** Renders that arrived before their descriptor's effect. */
const pendingRenders = new Map();

/**
 * Activate the extension.
 *
 * @param {vscode.ExtensionContext} context
 */
function activate(context) {
  output = vscode.window.createOutputChannel('magit-lsp');
  context.subscriptions.push(output);

  documentsChanged = new vscode.EventEmitter();
  context.subscriptions.push(documentsChanged);

  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(SCHEME, {
      onDidChange: documentsChanged.event,
      provideTextDocumentContent(uri) {
        const state = stateForUri(uri);
        return state ? state.lines.join('\n') : '';
      },
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('magit.open', openStatus),
    vscode.commands.registerCommand('magit.key', pressCommand),
    vscode.commands.registerCommand('magit.invoke', invokeCommand),
    vscode.commands.registerCommand('magit.mirrorFoldShortcuts', mirrorFoldShortcuts),
    vscode.workspace.onDidCloseTextDocument(onDocumentClosed),
    vscode.window.onDidChangeActiveTextEditor(onActiveEditorChanged)
  );

  client = createClient();
  client.onNotification('view/render', applyRender);
  client.onNotification('view/prompt', handlePrompt);
  clientReady = client.start().catch((error) => {
    output.appendLine(`language client failed: ${message(error)}`);
    vscode.window.showErrorMessage(`magit-lsp: ${message(error)}`);
  });

  activateView(undefined);
}

/**
 * Stop the language client when the extension deactivates.
 */
async function deactivate() {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

/**
 * Create the LSP client with the static key-delivery capabilities.
 *
 * @returns {LanguageClient}
 */
function createClient() {
  const config = vscode.workspace.getConfiguration('magit-lsp');
  const command = config.get('serverPath') || 'magit-lsp';
  const profile = config.get('profile') || 'standard';

  return new LanguageClient(
    'magit-lsp',
    'magit-lsp',
    { command, args: [] },
    {
      documentSelector: [{ scheme: SCHEME }],
      initializationOptions: { profile },
      middleware: {
        initialize(params, second, third) {
          params.capabilities = params.capabilities || {};
          params.capabilities.keymapDelivery = 'static';
          params.capabilities.keyModifiers = 'full';
          const next = typeof second === 'function' ? second : third;
          return next(params);
        },
      },
    }
  );
}

/**
 * The view state that owns a virtual document.
 *
 * @param {vscode.Uri} uri
 * @returns {object|undefined}
 */
function stateForUri(uri) {
  const id = uriToView.get(uri.toString());
  return id ? views.get(id) : undefined;
}

/**
 * The view state that currently has focus.
 *
 * @returns {object|undefined}
 */
function activeState() {
  return activeViewId ? views.get(activeViewId) : undefined;
}

/**
 * Whether a descriptor describes a menu view.
 *
 * @param {string} kind
 * @returns {boolean}
 */
function isMenuKind(kind) {
  return typeof kind === 'string' && kind.startsWith('menu.');
}

/**
 * Store a descriptor and its virtual document, replaying any early render.
 *
 * @param {object} descriptor
 * @returns {object|undefined}
 */
function ensureView(descriptor) {
  if (!descriptor || !descriptor.id) {
    return undefined;
  }

  const existing = views.get(descriptor.id);
  if (existing) {
    existing.descriptor = descriptor;
    return existing;
  }

  const uri = vscode.Uri.parse(descriptor.uri);
  const state = {
    id: descriptor.id,
    descriptor,
    uri,
    lines: [],
    version: undefined,
    mode: descriptor.mode,
    menu: undefined,
    cursor: undefined,
    quickPick: undefined,
    quickPickHiding: false,
    pressQueue: Promise.resolve(),
  };
  views.set(descriptor.id, state);
  uriToView.set(uri.toString(), descriptor.id);
  documentsChanged.fire(uri);

  const pending = pendingRenders.get(descriptor.id);
  if (pending) {
    pendingRenders.delete(descriptor.id);
    applyRender(pending);
  }
  return state;
}

/**
 * Open or focus the window for a view descriptor.
 *
 * @param {object} descriptor
 */
async function showView(descriptor) {
  const state = ensureView(descriptor);
  if (!state) {
    return;
  }
  if (isMenuKind(descriptor.kind) || state.menu) {
    showMenu(state);
    return;
  }
  await showDocument(state);
}

/**
 * Show a non-menu view as a virtual document.
 *
 * @param {object} state
 */
async function showDocument(state) {
  const document = await vscode.workspace.openTextDocument(state.uri);
  try {
    await vscode.languages.setTextDocumentLanguage(document, 'magit');
  } catch (error) {
    output.appendLine(`setTextDocumentLanguage failed: ${message(error)}`);
  }
  await vscode.window.showTextDocument(document, { preview: false, preserveFocus: false });
  if (state.id === activeViewId) {
    applyCursor(state, state.cursor);
  }
}

/**
 * Show a menu view as a QuickPick.
 *
 * Typed characters are forwarded one key step at a time; the server owns the
 * `-`/`=` prefixes, chords and the pop keys, so the input is cleared after
 * each step.
 *
 * @param {object} state
 */
function showMenu(state) {
  if (state.quickPick) {
    updateQuickPick(state);
    state.quickPick.show();
    activateView(state);
    return;
  }

  const quickPick = vscode.window.createQuickPick();
  quickPick.title = state.menu ? state.menu.title : state.descriptor.title;
  quickPick.placeholder = 'Press a menu key';
  quickPick.matchOnDescription = true;
  quickPick.matchOnDetail = true;
  quickPick.items = menuItems(state.menu);
  state.quickPick = quickPick;
  quickPickViewId = state.id;

  quickPick.onDidChangeValue((value) => {
    if (!value) {
      return;
    }
    const steps = menu.routeTyped(value);
    // Clearing the input raises one empty change event, which is ignored.
    quickPick.value = '';
    queueSteps(state, steps);
  });

  quickPick.onDidAccept(() => {
    const picked = quickPick.selectedItems && quickPick.selectedItems[0];
    if (picked && picked.key) {
      queueSteps(state, menu.routeTyped(picked.key));
    }
  });

  quickPick.onDidHide(() => {
    if (state.quickPickHiding) {
      return;
    }
    state.quickPickHiding = true;
    // Escape in the QuickPick never reaches a keybinding; tell the server to
    // pop the menu so the view stack stays in sync.
    queueSteps(state, ['Escape']);
  });

  quickPick.show();
  activateView(state);
}

/**
 * Refresh the QuickPick from the latest structured menu model.
 *
 * @param {object} state
 */
function updateQuickPick(state) {
  if (!state.quickPick) {
    return;
  }
  state.quickPick.title = state.menu ? state.menu.title : state.descriptor.title;
  state.quickPick.items = menuItems(state.menu);
}

/**
 * Convert the pure menu rows to VSCode QuickPick items.
 *
 * @param {object|undefined} model
 * @returns {vscode.QuickPickItem[]}
 */
function menuItems(model) {
  return menu.quickPickItems(model).map((row) => {
    if (row.kind === 'separator') {
      return { label: row.label, kind: vscode.QuickPickItemKind.Separator };
    }
    return {
      label: row.label,
      description: row.description,
      detail: row.roadmapId ? `planned: ${row.roadmapId}` : row.reason,
      kind: vscode.QuickPickItemKind.Default,
      key: row.key,
      itemKind: row.itemKind,
      status: row.status,
    };
  });
}

/**
 * Hide and dispose a menu QuickPick.
 *
 * @param {object} state
 */
function hideMenu(state) {
  if (!state.quickPick) {
    return;
  }
  state.quickPickHiding = true;
  state.quickPick.hide();
  state.quickPick.dispose();
  state.quickPick = undefined;
  if (quickPickViewId === state.id) {
    quickPickViewId = undefined;
  }
}

/**
 * Serialise key steps for one view so order is preserved.
 *
 * @param {object} state
 * @param {string[]} steps
 */
function queueSteps(state, steps) {
  state.pressQueue = state.pressQueue
    .then(async () => {
      for (const step of steps) {
        if (!views.has(state.id)) {
          return;
        }
        await press(state.id, step, { cursor: { line: 0, column: 0 } });
        if (menu.isPopKey(step)) {
          return;
        }
      }
    })
    .catch((error) => {
      output.appendLine(`key failed: ${message(error)}`);
    });
}

/**
 * Send one canonical key step through `view/key`.
 *
 * @param {string} viewId
 * @param {string} key
 * @param {{cursor: {line: number, column: number}, selection?: {startLine: number, endLine: number}}} info
 */
async function press(viewId, key, info) {
  const state = views.get(viewId);
  if (!state || !client) {
    return;
  }

  const params = {
    viewId,
    version: state.version || 0,
    key,
    editorMode: info.selection ? 'visual' : 'normal',
    cursor: info.cursor,
  };
  if (info.selection) {
    params.selection = info.selection;
  }

  try {
    await clientReady;
    const response = await client.sendRequest('view/key', params);
    await applyEffects(response && response.effects);
  } catch (error) {
    output.appendLine(`view/key ${key} failed: ${message(error)}`);
    vscode.window.showWarningMessage(`magit-lsp: ${message(error)}`);
  }
}

/**
 * Run a command through `view/invoke`.
 *
 * @param {string} viewId
 * @param {string} command
 * @param {object|string|undefined} args the command args, if any
 * @param {{cursor: {line: number, column: number}, selection?: object}} info
 */
async function invoke(viewId, command, args, info) {
  const state = views.get(viewId);
  if (!state || !client) {
    return;
  }

  const params = {
    viewId,
    command,
    cursor: info.cursor,
  };
  if (info.selection) {
    params.selection = info.selection;
  }
  if (args && typeof args === 'object' && args.args !== undefined) {
    params.args = args.args;
  }

  try {
    await clientReady;
    const response = await client.sendRequest('view/invoke', params);
    await applyEffects(response && response.effects);
  } catch (error) {
    output.appendLine(`view/invoke ${command} failed: ${message(error)}`);
    vscode.window.showWarningMessage(`magit-lsp: ${message(error)}`);
  }
}

/**
 * Apply a `view/render` notification, then acknowledge the version.
 *
 * @param {object} params
 */
function applyRender(params) {
  if (!params || !params.viewId) {
    return;
  }

  const state = views.get(params.viewId);
  if (!state) {
    pendingRenders.set(params.viewId, params);
    return;
  }

  const result = render.applyRender(state.lines, state.version, params);
  if (result.status === 'resync') {
    void resync(state.id);
    return;
  }

  state.lines = result.lines;
  if (result.status === 'full' || result.status === 'edits') {
    state.version = result.version;
  }
  if (params.mode) {
    state.mode = params.mode;
  }
  if (params.menu) {
    state.menu = params.menu;
  }
  if (params.cursor) {
    state.cursor = params.cursor;
  }

  documentsChanged.fire(state.uri);
  if (state.menu || isMenuKind(state.descriptor.kind)) {
    updateQuickPick(state);
  }
  if (state.id === activeViewId) {
    activateView(state);
  }
  if (params.cursor) {
    applyCursor(state, params.cursor);
  }
  if (client) {
    client.sendNotification('view/rendered', {
      viewId: state.id,
      version: params.version,
    });
  }
}

/**
 * Ask the user a server prompt and answer it with `view/promptRespond`.
 *
 * @param {object} params `view/prompt` params
 */
async function handlePrompt(params) {
  if (!params || !params.promptId || !client) {
    return;
  }

  let answer = { cancelled: true };
  try {
    answer = await promptFor(params);
  } catch (error) {
    output.appendLine(`view/prompt "${params.title}" failed: ${message(error)}`);
  }

  const request = { promptId: params.promptId, cancelled: answer.cancelled };
  if (!answer.cancelled) {
    request.value = answer.value;
  }
  try {
    await clientReady;
    await client.sendRequest('view/promptRespond', request);
  } catch (error) {
    output.appendLine(`view/promptRespond failed: ${message(error)}`);
  }
}

/**
 * Show one prompt with the matching VSCode widget.
 *
 * @param {object} params `view/prompt` params
 * @returns {Promise<{cancelled: boolean, value?: unknown, custom?: boolean}>}
 */
async function promptFor(params) {
  if (params.kind === 'text') {
    const value = await vscode.window.showInputBox({
      title: params.title,
      prompt: params.detail,
      placeHolder: params.placeholder,
      value: typeof params.default === 'string' ? params.default : undefined,
      password: Boolean(params.secret),
      ignoreFocusOut: true,
    });
    return prompt.answerFor(params, value);
  }

  if (params.kind === 'confirm' || params.kind === 'choice') {
    const picked = await vscode.window.showQuickPick(prompt.promptItems(params), {
      title: params.title,
      placeHolder: params.detail || params.title,
      ignoreFocusOut: true,
    });
    const answer = prompt.answerFor(params, picked);
    if (!answer.custom) {
      return answer;
    }
    const text = await vscode.window.showInputBox({
      title: params.title,
      placeHolder: params.placeholder || 'Value',
      ignoreFocusOut: true,
    });
    return prompt.customChoice(text);
  }

  if (params.kind === 'multiChoice') {
    const preselected = new Set(params.preselected || []);
    const items = prompt.promptItems(params).map((row) => ({
      ...row,
      picked: preselected.has(row.id),
    }));
    const picked = await vscode.window.showQuickPick(items, {
      title: params.title,
      placeHolder: params.detail || params.title,
      canPickMany: true,
      ignoreFocusOut: true,
    });
    return prompt.answerFor(params, picked);
  }

  // An unknown kind cannot be repaired client-side; cancel it.
  return { cancelled: true };
}

/**
 * Ask the server for the full text after a lost update.
 *
 * @param {string} viewId
 */
async function resync(viewId) {
  if (!client) {
    return;
  }
  try {
    await clientReady;
    const result = await client.sendRequest('view/resync', { viewId });
    if (result) {
      applyRender(result);
    }
  } catch (error) {
    output.appendLine(`view/resync failed: ${message(error)}`);
  }
}

/**
 * Move the cursor to the server's successor position.
 *
 * @param {object} state
 * @param {{line: number, column: number}|undefined} cursor
 */
function applyCursor(state, cursor) {
  if (!cursor) {
    return;
  }
  const editor = vscode.window.visibleTextEditors.find(
    (candidate) => candidate.document.uri.toString() === state.uri.toString()
  );
  if (!editor) {
    return;
  }
  const line = Math.min(cursor.line || 0, Math.max(0, editor.document.lineCount - 1));
  const text = editor.document.lineAt(line).text;
  const character = Math.min(cursor.column || 0, text.length);
  const position = new vscode.Position(line, character);
  editor.selection = new vscode.Selection(position, position);
  editor.revealRange(
    new vscode.Range(position, position),
    vscode.TextEditorRevealType.InCenterIfOutsideViewport
  );
}

/**
 * Set the `magit.*` context keys for a focused view.
 *
 * @param {object|undefined} state
 */
function activateView(state) {
  activeViewId = state ? state.id : undefined;
  const values = state
    ? contextKeys.deriveContext(state.descriptor, state.mode)
    : contextKeys.inactiveContext();
  for (const [key, value] of Object.entries(values)) {
    void vscode.commands.executeCommand('setContext', key, value);
  }
}

/**
 * Close a view's QuickPick/document and optionally focus the previous view.
 *
 * @param {string} viewId
 * @param {boolean} focusPrevious
 */
async function closeView(viewId, focusPrevious) {
  const state = views.get(viewId);
  if (!state) {
    return;
  }

  hideMenu(state);
  views.delete(viewId);
  uriToView.delete(state.uri.toString());
  documentsChanged.fire(state.uri);

  if (activeViewId !== viewId) {
    return;
  }
  activeViewId = undefined;

  if (focusPrevious) {
    const previous = Array.from(views.values()).pop();
    if (previous) {
      await focusView(previous.id);
      return;
    }
  }
  activateView(undefined);
}

/**
 * Focus an already-open view.
 *
 * @param {string} viewId
 */
async function focusView(viewId) {
  const state = views.get(viewId);
  if (!state) {
    return;
  }
  if (isMenuKind(state.descriptor.kind) || state.menu) {
    showMenu(state);
    return;
  }
  activateView(state);
  await showDocument(state);
}

/**
 * Apply the effects a request returned.
 *
 * @param {object[]|undefined} effects
 */
async function applyEffects(effects) {
  for (const effect of effects || []) {
    switch (effect.type) {
      case 'message':
        showMessage(effect);
        break;
      case 'clipboard':
        await vscode.env.clipboard.writeText(effect.text || '');
        vscode.window.showInformationMessage('Copied to clipboard.');
        break;
      case 'passThrough':
        await replay(effect.key);
        break;
      case 'openView':
      case 'pushView':
        await showView(effect.descriptor);
        break;
      case 'popView':
        await closeView(effect.viewId, true);
        break;
      case 'focusView':
        await focusView(effect.viewId);
        break;
      default:
        output.appendLine(`unknown effect: ${effect.type}`);
    }
  }
}

/**
 * Show a message effect at its level.
 *
 * @param {object} effect
 */
function showMessage(effect) {
  const text = effect.hint ? `${effect.text}\n${effect.hint}` : effect.text || '';
  if (effect.detail) {
    output.appendLine(effect.detail);
  }
  if (effect.level === 'error') {
    vscode.window.showErrorMessage(text);
  } else if (effect.level === 'warn') {
    vscode.window.showWarningMessage(text);
  } else {
    vscode.window.showInformationMessage(text);
  }
}

const REPLAY_COMMANDS = {
  'ctrl+c': 'editor.action.clipboardCopyAction',
  'ctrl+x': 'editor.action.clipboardCutAction',
  'ctrl+v': 'editor.action.clipboardPasteAction',
};

/**
 * Perform an editor behaviour the server passed through.
 *
 * A static client cannot translate arbitrary chords into VSCode commands, so
 * printable characters and clipboard shortcuts are the supported cases.
 *
 * @param {string} key
 */
async function replay(key) {
  if (REPLAY_COMMANDS[key]) {
    await vscode.commands.executeCommand(REPLAY_COMMANDS[key]);
    return;
  }
  if (key === 'Space') {
    await vscode.commands.executeCommand('default:type', { text: ' ' });
    return;
  }
  if (typeof key === 'string' && Array.from(key).length === 1) {
    await vscode.commands.executeCommand('default:type', { text: key });
    return;
  }
  output.appendLine(`passThrough: cannot replay ${key} from a static binding`);
}

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
      startLine: Math.min(selection.anchor.line, selection.active.line),
      endLine: Math.max(selection.anchor.line, selection.active.line),
    },
  };
}

/**
 * Tell the server when the user closes a view's document.
 *
 * @param {vscode.TextDocument} document
 */
async function onDocumentClosed(document) {
  const state = stateForUri(document.uri);
  if (!state) {
    return;
  }
  const viewId = state.id;
  await closeView(viewId, false);
  if (client) {
    await clientReady;
    client.sendNotification('view/close', { viewId });
  }
}

/**
 * React to focus moving into or out of a magit document.
 *
 * @param {vscode.TextEditor|undefined} editor
 */
function onActiveEditorChanged(editor) {
  if (!editor) {
    return;
  }
  if (editor.document.uri.scheme !== SCHEME) {
    if (!quickPickViewId) {
      activateView(undefined);
    }
    return;
  }
  const state = stateForUri(editor.document.uri);
  if (state && state.id !== quickPickViewId) {
    activateView(state);
  }
}

/**
 * Open the status view for the first workspace folder.
 */
async function openStatus() {
  if (!client) {
    return;
  }
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    vscode.window.showErrorMessage('magit-lsp: open a repository folder first.');
    return;
  }

  try {
    await clientReady;
    const descriptor = await client.sendRequest('view/open', {
      kind: 'status',
      args: { repo_path: folders[0].uri.fsPath },
      viewport: viewport(),
    });
    await showView(descriptor);
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
  const state = activeState();
  if (!state) {
    return;
  }
  const key = args && args.key;
  if (!key) {
    return;
  }

  if (state.quickPick || isMenuKind(state.descriptor.kind)) {
    queueSteps(state, [key]);
    return;
  }

  state.pressQueue = state.pressQueue
    .then(() => press(state.id, key, editorInfo()))
    .catch((error) => {
      output.appendLine(`key failed: ${message(error)}`);
    });
}

/**
 * `magit.invoke`: run a command by ID.
 *
 * @param {{command?: string, args?: object}|string|undefined} args
 */
async function invokeCommand(args) {
  const state = activeState();
  if (!state) {
    return;
  }
  const command = typeof args === 'string' ? args : args && args.command;
  if (!command) {
    return;
  }

  const info = state.quickPick ? { cursor: { line: 0, column: 0 } } : editorInfo();
  state.pressQueue = state.pressQueue
    .then(() => invoke(state.id, command, args, info))
    .catch((error) => {
      output.appendLine(`invoke failed: ${message(error)}`);
    });
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

/**
 * A readable message for an unknown error value.
 *
 * @param {unknown} error
 * @returns {string}
 */
function message(error) {
  return error && error.message ? error.message : String(error);
}

module.exports = { activate, deactivate };
