'use strict';

const vscode = require('vscode');
const drafts = require('./drafts');
const views = require('./views');
const { SCHEME, store } = require('./store');
const { message } = require('./util');

/** Detach draft documents safely; close server-owned status views separately. */
async function onDocumentClosed(document) {
  if (await drafts.onClosed(document)) return;
  const state = views.stateForUri(document.uri);
  if (!state || state.changingLanguage) return;
  const viewId = state.id;
  await views.closeView(viewId, false);
  const client = store.client;
  if (!client) return;
  try {
    await store.clientReady;
    if (store.client === client) await client.sendRequest('view/close', { viewId });
  } catch (error) {
    store.output.appendLine(`view/close failed: ${message(error)}`);
    vscode.window.showWarningMessage(`magit-lsp: ${message(error)}`);
  }
}

/** Restore view or native draft contexts when editor focus changes. */
function onActiveEditorChanged(editor) {
  const draft = editor && drafts.forUri(editor.document.uri);
  if (draft) {
    views.activateView(undefined);
    void drafts.setContext(require('./edit').editingContext(draft.effect));
    return;
  }
  void drafts.setContext(require('./edit').inactiveContext());
  if (!editor || editor.document.uri.scheme !== SCHEME) {
    if (!store.quickPickViewId) views.activateView(undefined);
    return;
  }
  const state = views.stateForUri(editor.document.uri);
  if (state && state.id !== store.quickPickViewId) views.activateView(state);
}

/** Confirm the focused managed draft using the versioned barriers. */
async function confirmEditCommand() {
  const draft = drafts.current();
  if (draft) await drafts.confirm(draft);
}

/** Abort the focused managed draft explicitly. */
async function cancelEditCommand() {
  const draft = drafts.current();
  if (draft) await drafts.cancel(draft);
}

exports.onDocumentClosed = onDocumentClosed;
exports.onActiveEditorChanged = onActiveEditorChanged;
exports.confirmEditCommand = confirmEditCommand;
exports.cancelEditCommand = cancelEditCommand;
exports.currentEdit = drafts.current;
