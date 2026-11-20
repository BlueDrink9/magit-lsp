"use strict";

const vscode = require("vscode");
const { cancel, confirm, current, forUri, onClosed, setContext } = require("./drafts");
const { editingContext, inactiveContext } = require("./edit");
const { activateView, closeView, stateForUri } = require("./views");
const { SCHEME, store } = require("./store");
const { message } = require("./util");

/** Detach draft documents safely; close server-owned status views separately. */
async function onDocumentClosed(document) {
  if (await onClosed(document)) return;
  const state = stateForUri(document.uri);
  if (!state || state.changingLanguage) return;
  const viewId = state.id;
  await closeView(viewId, false);
  const client = store.client;
  if (!client) return;
  try {
    await store.clientReady;
    if (store.client === client) await client.sendRequest("view/close", { viewId });
  } catch (error) {
    store.output.appendLine(`view/close failed: ${message(error)}`);
    vscode.window.showWarningMessage(`magit-lsp: ${message(error)}`);
  }
}

/** Restore view or native draft contexts when editor focus changes. */
function onActiveEditorChanged(editor) {
  const draft = editor && forUri(editor.document.uri);
  if (draft) {
    activateView(undefined);
    void setContext(editingContext(draft.effect));
    return;
  }
  void setContext(inactiveContext());
  if (!editor || editor.document.uri.scheme !== SCHEME) {
    if (!store.quickPickViewId) activateView(undefined);
    return;
  }
  const state = stateForUri(editor.document.uri);
  if (state && state.id !== store.quickPickViewId) activateView(state);
}

/** Confirm the focused managed draft using the versioned barriers. */
async function confirmEditCommand() {
  const draft = current();
  if (draft) await confirm(draft);
}

/** Abort the focused managed draft explicitly. */
async function cancelEditCommand() {
  const draft = current();
  if (draft) await cancel(draft);
}

exports.onDocumentClosed = onDocumentClosed;
exports.onActiveEditorChanged = onActiveEditorChanged;
exports.confirmEditCommand = confirmEditCommand;
exports.cancelEditCommand = cancelEditCommand;
exports.currentEdit = current;
