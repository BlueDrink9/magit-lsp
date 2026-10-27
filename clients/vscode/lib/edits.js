'use strict';

const vscode = require('vscode');

const edit = require('./edit');
const requests = require('./requests');
const views = require('./views');
const { SCHEME, store } = require('./store');
const { message } = require('./util');

/**
 * Set several context keys in one call.
 *
 * @param {Record<string, unknown>} values
 */
async function setContext(values) {
  for (const [key, value] of Object.entries(values)) {
    await vscode.commands.executeCommand('setContext', key, value);
  }
}

/**
 * Open a Git-owned file for an `editDocument` effect.
 *
 * @param {object} effect
 */
async function openEdit(effect) {
  if (!edit.isEditEffect(effect)) {
    return;
  }
  const uri = vscode.Uri.file(effect.path);
  const document = await vscode.workspace.openTextDocument(uri);
  try {
    await vscode.languages.setTextDocumentLanguage(
      document,
      edit.languageForKind(effect.kind)
    );
  } catch (error) {
    store.output.appendLine(`setTextDocumentLanguage failed: ${message(error)}`);
  }
  await vscode.window.showTextDocument(document, { preview: false, preserveFocus: false });

  const state = { effect, uri, document, closing: false };
  store.edits.set(effect.editId, state);
  store.editUris.set(uri.toString(), effect.editId);
  store.activeEditId = effect.editId;
  await setContext(edit.editingContext(effect));
}

/**
 * The edit owning the focused editor, if any.
 *
 * @returns {object|undefined}
 */
function currentEdit() {
  const editor = vscode.window.activeTextEditor;
  if (editor) {
    const id = store.editUris.get(editor.document.uri.toString());
    if (id && store.edits.has(id)) {
      return store.edits.get(id);
    }
  }
  return undefined;
}

/**
 * The id of the status view that owns the session, used to answer an edit.
 *
 * @returns {string|undefined}
 */
function statusViewId() {
  for (const state of store.views.values()) {
    if (!views.isMenuKind(state.descriptor.kind) && state.descriptor.scope === 'status') {
      return state.id;
    }
  }
  return store.activeViewId;
}

/**
 * Send `common.confirm` or `common.cancel` for an edit.
 *
 * @param {string} command
 * @param {string} editId
 */
async function answerEdit(command, editId) {
  const viewId = statusViewId();
  if (!viewId) {
    store.output.appendLine(`cannot answer edit ${editId}: no status view`);
    return;
  }
  await requests.invoke(viewId, command, edit.answerArgs(editId), {
    cursor: { line: 0, column: 0 },
  });
}

/**
 * Save the edit and let Git continue.
 */
async function confirmEditCommand() {
  const state = currentEdit();
  if (!state) {
    return;
  }
  try {
    await state.document.save();
  } catch (error) {
    store.output.appendLine(`saving the edit failed: ${message(error)}`);
    vscode.window.showWarningMessage(`magit-lsp: ${message(error)}`);
    return;
  }
  await answerEdit('common.confirm', state.effect.editId);
  await closeEdit(state);
}

/**
 * Abort the edit, leaving Git's operation undone.
 */
async function cancelEditCommand() {
  const state = currentEdit();
  if (!state) {
    return;
  }
  await answerEdit('common.cancel', state.effect.editId);
  await closeEdit(state);
}

/**
 * Forget an edit and close its document.
 *
 * @param {object} state
 */
async function closeEdit(state) {
  if (state.closing) {
    return;
  }
  state.closing = true;
  store.edits.delete(state.effect.editId);
  store.editUris.delete(state.uri.toString());
  if (store.activeEditId === state.effect.editId) {
    store.activeEditId = undefined;
  }
  await setContext(edit.inactiveContext());
  const tab = vscode.window.tabGroups.all
    .flatMap((group) => group.tabs)
    .find((candidate) => candidate.input && candidate.input.uri
      && candidate.input.uri.toString() === state.uri.toString());
  if (tab) {
    await vscode.window.tabGroups.close(tab);
  }
  const active = views.activeState();
  if (active) {
    views.activateView(active);
  }
}

/**
 * Request server cleanup asynchronously when the user closes a view's document.
 *
 * @param {vscode.TextDocument} document
 */
async function onDocumentClosed(document) {
  const editId = store.editUris.get(document.uri.toString());
  if (editId) {
    const editState = store.edits.get(editId);
    if (editState && !editState.closing) {
      // Closing the tab without confirming is a cancel (§3.10).
      await answerEdit('common.cancel', editId);
      await closeEdit(editState);
    }
    return;
  }

  const state = views.stateForUri(document.uri);
  if (!state || state.changingLanguage) {
    return;
  }
  const viewId = state.id;
  await views.closeView(viewId, false);
  const client = store.client;
  if (client) {
    try {
      await store.clientReady;
      if (store.client !== client) return;
      await client.sendRequest('view/close', { viewId });
    } catch (error) {
      store.output.appendLine(`view/close failed: ${message(error)}`);
      vscode.window.showWarningMessage(`magit-lsp: ${message(error)}`);
    }
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
    if (!store.quickPickViewId) {
      views.activateView(undefined);
    }
    return;
  }
  const state = views.stateForUri(editor.document.uri);
  if (state && state.id !== store.quickPickViewId) {
    views.activateView(state);
  }
}

exports.setContext = setContext;
exports.openEdit = openEdit;
exports.currentEdit = currentEdit;
exports.statusViewId = statusViewId;
exports.answerEdit = answerEdit;
exports.confirmEditCommand = confirmEditCommand;
exports.cancelEditCommand = cancelEditCommand;
exports.closeEdit = closeEdit;
exports.onDocumentClosed = onDocumentClosed;
exports.onActiveEditorChanged = onActiveEditorChanged;
