'use strict';

const vscode = require('vscode');

const render = require('./render');
const prompt = require('./prompt');
const effects = require('./effects');
const menus = require('./menus');
const views = require('./views');
const { store } = require('./store');
const { message } = require('./util');

/**
 * Send one canonical key step through `view/key`.
 *
 * @param {string} viewId
 * @param {string} key
 * @param {{cursor: {line: number, column: number}, selection?: {startLine: number, endLine: number}}} info
 */
async function press(viewId, key, info) {
  const state = store.views.get(viewId);
  const client = store.client;
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
    await store.clientReady;
    const response = await client.sendRequest('view/key', params);
    await effects.applyEffects(response && response.effects);
  } catch (error) {
    store.output.appendLine(`view/key ${key} failed: ${message(error)}`);
    vscode.window.showWarningMessage(`magit-lsp: ${message(error)}`);
  }
}

/**
 * Run a command through `view/invoke`.
 *
 * @param {string} viewId
 * @param {string} command
 * @param {unknown} args the command args, if any (not the VSCode envelope)
 * @param {{cursor: {line: number, column: number}, selection?: object}} info
 */
async function invoke(viewId, command, args, info) {
  const state = store.views.get(viewId);
  const client = store.client;
  if (!state || !client) {
    return;
  }

  const params = {
    viewId,
    command,
    version: state.version,
    cursor: info.cursor,
  };
  if (info.selection) {
    params.selection = info.selection;
  }
  if (args !== undefined) {
    params.args = args;
  }

  try {
    await store.clientReady;
    const response = await client.sendRequest('view/invoke', params);
    await effects.applyEffects(response && response.effects);
  } catch (error) {
    store.output.appendLine(`view/invoke ${command} failed: ${message(error)}`);
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

  const state = store.views.get(params.viewId);
  if (!state) {
    store.pendingRenders.set(params.viewId, params);
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
    if (state.resolveFirstRender) {
      state.resolveFirstRender();
      state.resolveFirstRender = undefined;
    }
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

  store.documentsChanged.fire(state.uri);
  if (state.menu || views.isMenuKind(state.descriptor.kind)) {
    menus.updateQuickPick(state);
  }
  if (state.id === store.activeViewId) {
    views.activateView(state);
  }
  if (params.cursor) {
    views.applyCursor(state, params.cursor);
  }
  const client = store.client;
  if (client) {
    client.sendNotification('view/rendered', {
      viewId: state.id,
      version: params.version,
    });
    store.semanticChanged?.fire();
  }
}

/**
 * Ask the user a server prompt and answer it with `view/promptRespond`.
 *
 * @param {object} params `view/prompt` params
 */
async function handlePrompt(params) {
  if (!params || !params.promptId || !store.client) {
    return;
  }

  let answer = { cancelled: true };
  try {
    answer = await promptFor(params);
  } catch (error) {
    store.output.appendLine(`view/prompt "${params.title}" failed: ${message(error)}`);
  }

  const request = { promptId: params.promptId, cancelled: answer.cancelled };
  if (!answer.cancelled) {
    request.value = answer.value;
  }
  try {
    await store.clientReady;
    await store.client.sendRequest('view/promptRespond', request);
  } catch (error) {
    store.output.appendLine(`view/promptRespond failed: ${message(error)}`);
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
  const client = store.client;
  if (!client) {
    return;
  }
  try {
    await store.clientReady;
    const result = await client.sendRequest('view/resync', { viewId });
    if (result) {
      applyRender(result);
    }
  } catch (error) {
    store.output.appendLine(`view/resync failed: ${message(error)}`);
  }
}

exports.press = press;
exports.invoke = invoke;
exports.applyRender = applyRender;
exports.handlePrompt = handlePrompt;
exports.promptFor = promptFor;
exports.resync = resync;
