'use strict';

const vscode = require('vscode');

const contextKeys = require('./context');
const menus = require('./menus');
const requests = require('./requests');
const { SCHEME, store } = require('./store');
const { message } = require('./util');

/**
 * The view state that owns a virtual document.
 *
 * @param {vscode.Uri} uri
 * @returns {object|undefined}
 */
function stateForUri(uri) {
  const id = store.uriToView.get(uri.toString());
  return id ? store.views.get(id) : undefined;
}

/**
 * The view state that currently has focus.
 *
 * @returns {object|undefined}
 */
function activeState() {
  return store.activeViewId ? store.views.get(store.activeViewId) : undefined;
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

  const existing = store.views.get(descriptor.id);
  if (existing) {
    existing.descriptor = descriptor;
    return existing;
  }

  // Keep the server URI as the identity, but use a stable, titled VSCode URI
  // so the editor never presents the repository path as a scratch filename.
  const uri = vscode.Uri.from({
    scheme: SCHEME,
    authority: descriptor.id,
    path: `/${encodeURIComponent(descriptor.title || descriptor.kind || 'Magit')}`,
  });
  let resolveFirstRender;
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
    firstRender: new Promise((resolve) => { resolveFirstRender = resolve; }),
    resolveFirstRender: undefined,
  };
  state.resolveFirstRender = resolveFirstRender;
  store.views.set(descriptor.id, state);
  store.uriToView.set(uri.toString(), descriptor.id);
  store.documentsChanged.fire(uri);

  const pending = store.pendingRenders.get(descriptor.id);
  if (pending) {
    store.pendingRenders.delete(descriptor.id);
    requests.applyRender(pending);
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
    menus.showMenu(state);
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
  // `view/open` returns its descriptor before the first `view/render` can
  // arrive. Do not open the provider document until its initial text is ready.
  await state.firstRender;
  const document = await vscode.workspace.openTextDocument(state.uri);
  state.changingLanguage = true;
  try {
    await vscode.languages.setTextDocumentLanguage(document, 'magit');
  } catch (error) {
    store.output.appendLine(`setTextDocumentLanguage failed: ${message(error)}`);
  } finally {
    state.changingLanguage = false;
  }
  await vscode.window.showTextDocument(document, { preview: false, preserveFocus: false });
  activateView(state);
  if (state.id === store.activeViewId) {
    applyCursor(state, state.cursor);
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
  store.activeViewId = state ? state.id : undefined;
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
  const state = store.views.get(viewId);
  if (!state) {
    return;
  }

  menus.hideMenu(state);
  store.views.delete(viewId);
  store.uriToView.delete(state.uri.toString());
  store.documentsChanged.fire(state.uri);

  if (store.activeViewId !== viewId) {
    return;
  }
  store.activeViewId = undefined;

  if (focusPrevious) {
    const previous = Array.from(store.views.values()).pop();
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
  const state = store.views.get(viewId);
  if (!state) {
    return;
  }
  if (isMenuKind(state.descriptor.kind) || state.menu) {
    menus.showMenu(state);
    return;
  }
  activateView(state);
  await showDocument(state);
}

exports.stateForUri = stateForUri;
exports.activeState = activeState;
exports.isMenuKind = isMenuKind;
exports.ensureView = ensureView;
exports.showView = showView;
exports.showDocument = showDocument;
exports.applyCursor = applyCursor;
exports.activateView = activateView;
exports.closeView = closeView;
exports.focusView = focusView;
