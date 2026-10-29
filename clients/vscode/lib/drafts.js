'use strict';

/**
 * Native commit-message files for VSCode (§3.10).
 *
 * The server owns the draft state, Git's operation and the versioned barriers.
 * This thin adapter opens Git's file, owns one full-sync stream and preserves
 * exact-version confirmation/history barriers. Only an actual last owned tab
 * closed event confirms saved disk text; document-close is never confirmation.
 */

const vscode = require('vscode');

const edit = require('./edit');
const views = require('./views');
const { store } = require('./store');
const { message } = require('./util');

/** Maximum live or saved UTF-8 draft size (§3.10). */
const MAX_BYTES = 1024 * 1024;


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
 * The draft record for a URI, if it is still live.
 *
 * @param {vscode.Uri} uri
 * @returns {object|undefined}
 */
function forUri(uri) {
  const runId = store.draftUris.get(uri.toString());
  return runId ? store.drafts.get(runId) : undefined;
}

/**
 * Whether a record still owns its document and has not begun teardown.
 *
 * @param {object|undefined} record
 * @returns {boolean}
 */
function live(record) {
  return Boolean(record)
    && store.drafts.get(record.runId) === record
    && !record.closing
    && !store.disposing
    && Boolean(record.document)
    && (!record.document.isClosed || record.nativeClosed);
}

/**
 * The draft owning the focused editor, if any.
 *
 * @returns {object|undefined}
 */
function current() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.uri.scheme !== edit.SCHEME) {
    return undefined;
  }
  const record = forUri(editor.document.uri);
  return live(record) ? record : undefined;
}

/**
 * The status view that owns the server session, preferring the origin view.
 *
 * @param {object} record
 * @returns {string|undefined}
 */
function baseViewId(record) {
  return record.baseViewId;
}

/**
 * Turn a server or transport error into user-facing prose.
 *
 * The adapter never shows raw JSON; an unexpected structured error falls back
 * to a readable summary.
 *
 * @param {unknown} error
 * @returns {string}
 */
function readable(error) {
  const text = message(error).trim();
  if (!text) {
    return 'the commit draft could not be synchronized';
  }
  if (text.startsWith('{') || text.startsWith('[') || text === '[object Object]') {
    return 'the commit draft could not be synchronized';
  }
  return text;
}

/**
 * Record a recoverable sync failure, retain the user's text and invalidate
 * in-flight barriers. It never closes the document or releases the helper.
 *
 * @param {object} record
 * @param {unknown} reason
 */
function fail(record, reason) {
  record.generation += 1;
  record.syncError = readable(reason);
  const text = `magit-lsp: commit draft: ${record.syncError}`
    + ' Your text is retained; retry saving or cancel the draft.';
  if (record.lastError !== text) {
    record.lastError = text;
    store.output.appendLine(text);
    vscode.window.showErrorMessage(text);
  }
}

/**
 * Clear a recoverable failure once a barrier succeeds.
 *
 * @param {object} record
 */
function recover(record) {
  record.syncError = undefined;
  record.lastError = undefined;
}

/**
 * Send one standard full-sync notification while the draft is live.
 *
 * @param {object} record
 * @param {string} method
 * @param {object} params
 * @returns {Promise<boolean>}
 */
async function notify(record, method, params, quiet) {
  const client = record.client;
  if (!client || store.client !== client || record.disconnected) {
    if (!quiet) fail(record, 'the draft connection is unavailable');
    return false;
  }
  try {
    await store.clientReady;
    if (store.client !== client || record.disconnected) {
      throw new Error('the draft connection was replaced');
    }
    await client.sendNotification(method, params);
    return true;
  } catch (error) {
    if (quiet) {
      store.output.appendLine(`commit draft ${method} failed: ${readable(error)}`);
    } else {
      fail(record, error);
    }
    return false;
  }
}

/**
 * Send one versioned barrier request, correlated only by its JSON-RPC id.
 *
 * @param {object} record
 * @param {string} method
 * @param {object} payload
 * @returns {Promise<object|undefined>}
 */
async function request(record, method, payload) {
  const client = record.client;
  const generation = record.generation;
  if (!client || store.client !== client || record.disconnected) {
    fail(record, 'the draft connection is unavailable');
    return undefined;
  }
  try {
    await store.clientReady;
    if (store.client !== client || !live(record) || record.disconnected) return undefined;
    const result = await client.sendRequest(method, payload);
    if (!live(record) || store.client !== client || record.disconnected || record.generation !== generation) {
      return undefined;
    }
    return result;
  } catch (error) {
    if (!live(record) || record.disconnected || record.generation !== generation) {
      return undefined;
    }
    fail(record, error);
    return undefined;
  }
}

/**
 * Attach the full-sync document, retrying a failed first open.
 *
 * @param {object} record
 * @returns {Promise<boolean>}
 */
async function attach(record) {
  if (record.attached) {
    return true;
  }
  const ok = await notify(record, 'textDocument/didOpen', {
    textDocument: {
      uri: record.uri.toString(),
      languageId: 'gitcommit',
      version: record.version,
      text: record.text,
    },
  });
  record.attached = ok;
  return ok;
}

/**
 * Flush the live document as one complete full-sync change, exactly once.
 *
 * @param {object} record
 * @returns {Promise<boolean>}
 */
async function flush(record) {
  if (!live(record) || record.disconnected) {
    return false;
  }
  if (!(await attach(record))) {
    return false;
  }
  const text = record.document.getText();
  if (Buffer.byteLength(text, 'utf8') > MAX_BYTES) {
    fail(record, 'the commit draft exceeds the 1 MiB limit; shorten it before saving');
    return false;
  }
  if (text !== record.text || record.syncError) {
    const version = record.version + 1;
    if (!Number.isSafeInteger(version)) {
      fail(record, 'the draft version limit was reached; copy your text and reopen');
      return false;
    }
    if (!(await notify(record, 'textDocument/didChange', {
      textDocument: { uri: record.uri.toString(), version },
      contentChanges: [{ text }],
    }))) {
      return false;
    }
    record.version = version;
    record.text = text;
  }
  return true;
}

/**
 * Serialize native edits, barriers and teardown for one draft.
 *
 * @param {object} record
 * @param {() => Promise<unknown>} action
 * @returns {Promise<unknown>}
 */
function enqueue(record, action) {
  const next = record.queue.then(action, action);
  record.queue = next.then(() => {}, () => {});
  return next;
}

/**
 * Wait until an in-flight barrier has finished.
 *
 * @param {object} record
 */
async function waitIdle(record) {
  for (let attempt = 0; attempt < 100 && record.busy
    && !record.closing && !record.disconnected; attempt += 1) {
    const queue = record.queue;
    await queue.catch(() => {});
    if (record.queue === queue && record.busy) {
      return;
    }
  }
}

/**
 * Reserve the draft for one externally triggered operation.
 *
 * @param {object} record
 * @returns {boolean}
 */
function begin(record) {
  if (!live(record) || record.busy || record.disconnected) {
    return false;
  }
  record.busy = true;
  return true;
}

/**
 * Release the draft after a completed operation.
 *
 * @param {object} record
 */
function unlock(record) {
  record.busy = false;
}

/**
 * Run one serialized operation while native edits are gated.
 *
 * @param {object} record
 * @param {() => Promise<boolean>} action
 * @returns {Promise<boolean>}
 */
async function operate(record, action) {
  await waitIdle(record);
  if (!begin(record)) {
    return false;
  }
  let ok = false;
  try {
    // Native editors are gated before awaiting history/checkpoint/finish. API
    // edits can still race, so every replacement also checks the native version.
    await vscode.commands.executeCommand('workbench.action.files.setActiveEditorReadonlyInSession', record.uri);
    await enqueue(record, async () => {
      ok = await action();
    });
  } catch (error) {
    if (live(record)) fail(record, error);
  } finally {
    await vscode.commands.executeCommand('workbench.action.files.resetActiveEditorReadonlyInSession', record.uri);
    unlock(record);
    if (!record.syncError && record.document?.getText() !== record.text) scheduleFlush(record);
  }
  return ok;
}

/**
 * Schedule one coalesced flush for a native document change.
 *
 * @param {object} record
 */
function scheduleFlush(record) {
  if (!live(record) || record.busy || record.applying || record.disconnected) {
    return;
  }
  if (record.flushQueued) {
    return;
  }
  record.flushQueued = true;
  enqueue(record, async () => {
    record.flushQueued = false;
    if (!live(record) || record.busy || record.applying || record.disconnected) {
      return;
    }
    await flush(record);
  }).catch(() => {});
}

/**
 * The result shape a barrier must acknowledge.
 *
 * @param {object|undefined} result
 * @param {string} type
 * @param {object} record
 * @param {number} version
 * @returns {boolean}
 */
function acknowledged(result, type, record, version) {
  return Boolean(result)
    && result.type === type
    && result.runId === record.runId
    && result.uri === record.uri.toString()
    && result.version === version
    && Object.keys(result).every((key) => ['type', 'runId', 'uri', 'version'].includes(key));
}

/**
 * Checkpoint the exact applied live version without confirming anything.
 *
 * @param {object} record
 * @returns {Promise<boolean>}
 */
async function checkpoint(record) {
  const version = record.version;
  const text = record.text;
  if (!(await record.document.save()) || record.document.getText() !== text) {
    fail(record, 'the commit message changed while saving');
    return false;
  }
  record.savedVersion = version;
  record.savedText = text;
  recover(record);
  return true;
}

/**
 * Close a draft's tab without treating the close event as user intent.
 *
 * @param {object} record
 */
async function closeTab(record) {
  const tabs = vscode.window.tabGroups.all
    .flatMap((group) => group.tabs)
    .filter((tab) => tab.input && tab.input.uri
      && tab.input.uri.toString() === record.uri.toString());
  if (tabs.length === 0) {
    return;
  }
  try {
    await vscode.window.tabGroups.close(tabs);
  } catch (error) {
    store.output.appendLine(`commit draft tab close failed: ${message(error)}`);
  }
}

/**
 * Forget a draft after its run is terminal and restore the owning view.
 *
 * @param {object} record
 */
async function forget(record) {
  if (record.closing) {
    return;
  }
  record.closing = true;
  if (store.activeDraftId === record.runId) {
    store.activeDraftId = undefined;
  }
  await setContext(edit.inactiveContext());
  record.attached = false;
  // Revert only after a positively acknowledged finish/abort, so the native
  // workbench does not offer a second Save dialog during terminal teardown.
  record.discarding = true;
  if (record.document && !record.document.isClosed) {
    await vscode.commands.executeCommand('workbench.action.files.revert', record.uri);
  }
  await closeTab(record);
  store.drafts.delete(record.runId);
  store.draftUris.delete(record.uri.toString());
  const viewId = baseViewId(record);
  if (viewId) {
    await views.focusView(viewId);
  } else {
    views.activateView(undefined);
  }
}

/**
 * Finish the named saved checkpoint, then close the document.
 *
 * @param {object} record
 * @param {'confirm'|'normal-close'} reason
 * @returns {Promise<boolean>}
 */
async function finish(record, reason) {
  const version = record.savedVersion;
  const result = await request(record, 'workspace/executeCommand', { command: 'magit.command', arguments: [{
    command: 'common.confirm',
    runId: record.runId,
    uri: record.uri.toString(),
    version: reason === 'confirm' ? record.version : version,
    source: reason === 'confirm' ? 'synchronized' : 'saved-file',
  }] });
  if (!result) {
    return false;
  }
  if (!acknowledged(result, 'draft/finished', record, reason === 'confirm' ? record.version : version)) {
    fail(record, 'unexpected commit draft finish acknowledgement');
    return false;
  }
  await forget(record);
  return true;
}

/**
 * Explicit confirmation: flush, checkpoint, await finish, then close.
 *
 * Save never reaches this path; only this barrier confirms.
 *
 * @param {object} record
 */
async function confirm(record) {
  return operate(record, async () => {
    if (!(await flush(record))) return false;
    if (record.document.getText() === record.text) {
      return finish(record, 'confirm');
    }
    fail(record, 'the commit draft changed while confirming; save or retry confirm');
    return false;
  });
}

/**
 * Native save: checkpoint only, keep the editor and Git's helper pending.
 *
 * @param {object} record
 * @returns {Promise<boolean>}
 */
async function save(record, expectedText) {
  return operate(record, async () => {
    if (!(await flush(record))) {
      return false;
    }
    if (expectedText !== undefined && record.text !== expectedText) {
      fail(record, 'the draft changed before saving; retry the save');
      return false;
    }
    return checkpoint(record);
  });
}

/**
 * Explicit normal close: confirm the saved checkpoint, or ask Save/Discard/Cancel.
 *
 * @param {object} record
 */
async function close(record) {
  await operate(record, async () => {
    if (!(await flush(record))) {
      return false;
    }
    const clean = !record.syncError && record.text === record.savedText;
    if (clean) {
      return finish(record, 'normal-close');
    }
    const choice = await vscode.window.showQuickPick(
      [
        { id: 'save', label: 'Save' },
        { id: 'discard', label: 'Discard changes' },
        { id: 'cancel', label: 'Cancel' },
      ],
      { title: 'Close commit draft', placeHolder: 'Save, discard changes or cancel' }
    );
    if (!choice || choice.id === 'cancel') {
      return false;
    }
    if (choice.id === 'save') {
      if (!(await flush(record))) return false;
      if (!(await checkpoint(record))) {
        return false;
      }
      return finish(record, 'normal-close');
    }
    // A rejected sync must never release the helper, even when discarding.
    if (record.syncError) {
      fail(record, record.syncError);
      return false;
    }
    return finish(record, 'normal-close');
  });
}

/**
 * Abort explicitly through the server session, then close the document.
 *
 * @param {object} record
 */
async function cancel(record) {
  if (!live(record)) {
    return;
  }
  // Abort must not wait behind an in-flight finish/history barrier. Invalidate
  // callbacks first; the server owns finish-versus-cancel arbitration.
  record.generation += 1;
  record.closing = true;
  const viewId = baseViewId(record);
  if (viewId && !record.disconnected && store.client === record.client) {
    try {
      const result = await record.client.sendRequest('view/invoke', {
        viewId, command: 'common.cancel', args: { runId: record.runId },
        cursor: { line: 0, column: 0 },
      });
      await require('./effects').applyEffects(result && result.effects, viewId);
    } catch (error) {
      store.output.appendLine(`commit draft cancel failed: ${message(error)}`);
    }
  } else {
    store.output.appendLine('commit draft cancel found no server view');
  }
  record.closing = false;
  await forget(record);
}

/**
 * Replace the whole live document with server-owned recalled text.
 *
 * @param {object} record
 * @param {string} text
 */
async function replace(record, text, nativeBase) {
  const document = record.document;
  await vscode.commands.executeCommand('workbench.action.files.resetActiveEditorReadonlyInSession', record.uri);
  if (!live(record) || document.version !== nativeBase) return false;
  const lines = document.getText().split('\n');
  const end = new vscode.Position(lines.length - 1, lines[lines.length - 1].length);
  const replacement = new vscode.WorkspaceEdit();
  replacement.replace(document.uri, new vscode.Range(new vscode.Position(0, 0), end), text);
  const applied = await vscode.workspace.applyEdit(replacement);
  if (!applied) {
    throw new Error('the commit draft could not be replaced');
  }
  return true;
}

/**
 * Recall server-owned history at the exact applied base version.
 *
 * @param {object} record
 * @param {string} command
 */
async function history(record, command) {
  return operate(record, async () => {
    if (!(await flush(record))) {
      return false;
    }
    const base = record.version;
    const before = record.text;
    const document = record.document;
    const nativeBase = document.version;
    const result = await request(record, 'draft/history', {
      runId: record.runId,
      uri: record.uri.toString(),
      version: base,
      command,
    });
    if (!result) {
      return false;
    }
    if (result.type !== 'draft/replaced'
      || result.runId !== record.runId
      || result.uri !== record.uri.toString()
      || result.baseVersion !== base
      || result.version !== base + 1
      || typeof result.text !== 'string'
      || !Object.keys(result).every((key) => ['type', 'runId', 'uri', 'baseVersion', 'version', 'text'].includes(key))) {
      fail(record, 'unexpected commit draft history acknowledgement');
      return false;
    }
    record.version = result.version;
    if (record.document === document && document.version === nativeBase && document.getText() === before) {
      record.applying = true;
      try {
        await replace(record, result.text, nativeBase);
      } catch (error) {
        record.applying = false;
        fail(record, error);
        return false;
      }
      record.applying = false;
      record.text = result.text;
      recover(record);
      return flush(record);
    }
    // A native mutation raced the response: keep local text, forbid the stale
    // replacement and reconcile through full sync above the returned version.
    record.text = result.text;
    recover(record);
    return flush(record);
  });
}

/**
 * Open the `editDocument` effect's managed draft in the same window.
 *
 * @param {object} effect
 * @param {string|undefined} originViewId
 */
async function open(effect, originViewId) {
  if (!edit.isEditEffect(effect)) {
    vscode.window.showErrorMessage('magit-lsp: invalid managed commit draft announcement; no document was opened.');
    return;
  }
  const existing = store.drafts.get(effect.runId);
  if (existing) {
    if (live(existing)) {
      await vscode.window.showTextDocument(existing.document, { preview: false });
      await setContext(edit.editingContext(existing.effect));
    }
    return;
  }
  const uri = vscode.Uri.parse(effect.uri);
  const record = {
    runId: effect.runId,
    client: store.client,
    uri,
    effect,
    baseViewId: originViewId,
    document: undefined,
    text: effect.text,
    version: effect.version,
    savedVersion: effect.version,
    savedText: effect.text,
    attached: false,
    closing: false,
    applying: false,
    busy: false,
    disconnected: false,
    changingLanguage: false,
    syncError: undefined,
    lastError: undefined,
    generation: 0,
    flushQueued: false,
    queue: Promise.resolve(),
    mtime: Date.now(),
  };
  store.drafts.set(record.runId, record);
  store.draftUris.set(uri.toString(), record.runId);
  store.activeDraftId = record.runId;

  let document;
  try {
    document = await vscode.workspace.openTextDocument(uri);
    record.document = document;
    record.changingLanguage = true;
    try {
      await vscode.languages.setTextDocumentLanguage(
        document,
        edit.languageForKind(effect.kind)
      );
    } catch (error) {
      store.output.appendLine(`setTextDocumentLanguage failed: ${message(error)}`);
    } finally {
      record.changingLanguage = false;
    }
    // VSCode may close and reopen the document for a language change.
    document = await vscode.workspace.openTextDocument(uri);
    record.document = document;
    await vscode.window.showTextDocument(document, { preview: false, preserveFocus: false });
    record.document = document;
    record.tabs = new Set(vscode.window.tabGroups.all.flatMap((group) => group.tabs)
      .filter((tab) => tab.input?.uri?.toString() === record.uri.toString()));
  } catch (error) {
    store.output.appendLine(`commit draft document failed: ${message(error)}`);
    store.drafts.delete(record.runId);
    store.draftUris.delete(uri.toString());
    store.activeDraftId = undefined;
    vscode.window.showErrorMessage(`magit-lsp: cannot open the commit draft: ${readable(error)}`);
    return;
  }

  await setContext(edit.editingContext(effect));
  await attach(record);
  if (store.activeDraftId !== record.runId) {
    store.activeDraftId = record.runId;
  }
}

/**
 * Route a generated command while a managed draft has focus.
 *
 * @param {unknown} args the `magit.invoke` arguments
 * @returns {boolean} whether the draft owned the command
 */
async function handleInvoke(args) {
  const record = current();
  if (!record) {
    return false;
  }
  const command = typeof args === 'string' ? args : args && args.command;
  const action = edit.draftAction(command);
  if (action === 'confirm') {
    await confirm(record);
  } else if (action === 'cancel') {
    await cancel(record);
  } else if (action === 'close') {
    await close(record);
  } else if (action === 'history-prev' || action === 'history-next') {
    await history(record, edit.historyCommand(action));
  } else if (command) {
    store.output.appendLine(`commit draft ignored command: ${command}`);
  }
  return true;
}

/**
 * Apply one standard `didChange` full-sync flush after a native edit.
 *
 * @param {vscode.TextDocumentChangeEvent} event
 */
function onChanged(event) {
  const record = forUri(event.document.uri);
  if (!record || record.changingLanguage || !event.contentChanges.length) {
    return;
  }
  record.document = event.document;
  scheduleFlush(record);
}

/**
 * Handle a server `draft/error` for the matching live draft.
 *
 * @param {{uri?: string, message?: string}} notice
 */
function onError(notice) {
  if (!notice || !notice.uri) {
    return;
  }
  const runId = store.draftUris.get(notice.uri);
  const record = runId ? store.drafts.get(runId) : undefined;
  if (live(record)) {
    fail(record, notice.message || 'the commit draft was rejected');
  }
}

/**
 * Ignore document teardown as confirmation (including language changes).
 *
 * @param {vscode.TextDocument} document
 * @returns {boolean} whether the document belonged to a managed draft
 */
async function onClosed(document) {
  const runId = store.draftUris.get(document.uri.toString());
  if (!runId) {
    return false;
  }
  const record = store.drafts.get(runId);
  if (!record || record.closing) {
    return true;
  }
  if (record.changingLanguage) {
    return true;
  }
  if (store.disposing || record.disconnected) {
    return true;
  }
  // Document close is also emitted by language changes; only a real last-tab
  // event below supplies semantic saved-file confirmation.
  return true;
}

/**
 * Confirm saved disk text only for an actual last owned tab closure.
 *
 * Match tab identity as well as URI/run, and inspect every native tab group.
 * Focus switches, nonlast closes and stale events for reused paths cannot finish.
 *
 * @returns {void}
 */
function onTabsChanged(event) {
  if (store.disposing || !event?.closed?.length) return;
  const closed = new Set(event.closed.filter((tab) => tab.input?.uri)
    .map((tab) => tab.input.uri.toString()));
  const open = new Set(vscode.window.tabGroups.all
    .flatMap((group) => group.tabs)
    .filter((tab) => tab.input && tab.input.uri)
    .map((tab) => tab.input.uri.toString()));
  for (const record of store.drafts.values()) {
    const ownedClose = event.closed.some((tab) => record.tabs?.has(tab));
    record.tabs = new Set(vscode.window.tabGroups.all.flatMap((group) => group.tabs)
      .filter((tab) => tab.input?.uri?.toString() === record.uri.toString()));
    if (!ownedClose || !closed.has(record.uri.toString()) || !record.attached || record.closing
      || record.changingLanguage || record.disconnected) {
      continue;
    }
    if (open.has(record.uri.toString())) {
      continue;
    }
    record.nativeClosed = true;
    void enqueue(record, () => finish(record, 'normal-close'));
  }
}

/**
 * Register native save observation and actual tab-close confirmation.
 *
 * @param {vscode.ExtensionContext} context
 */
function setup(context) {
  store.disposing = false;
  if (vscode.workspace.onDidSaveTextDocument) {
    context.subscriptions.push(vscode.workspace.onDidSaveTextDocument((document) => {
      const record = forUri(document.uri);
      if (!live(record)) return;
      void enqueue(record, async () => {
        if (await flush(record)) {
          record.savedText = document.getText();
          record.savedVersion = record.version;
          recover(record);
        }
      });
    }));
  }
  // Virtual documents are retained after their tab closes, so a raw workbench
  // close never fires `onDidCloseTextDocument`; watch the tabs instead for the
  // ambiguous teardown that must cancel (§3.10).
  context.subscriptions.push(
    vscode.window.tabGroups.onDidChangeTabs(onTabsChanged)
  );
}

/**
 * Mark every live draft disconnected; text is retained but no barrier may run.
 *
 * @param {string} reason
 */
function disconnect(reason) {
  for (const record of store.drafts.values()) {
    record.disconnected = true;
    fail(record, reason);
  }
}

exports.cancel = cancel;
exports.close = close;
exports.confirm = confirm;
exports.current = current;
exports.disconnect = disconnect;
exports.forUri = forUri;
exports.handleInvoke = handleInvoke;
exports.history = history;
exports.onChanged = onChanged;
exports.onClosed = onClosed;
exports.onError = onError;
exports.onTabsChanged = onTabsChanged;
exports.open = open;
exports.save = save;
exports.setContext = setContext;
exports.setup = setup;
