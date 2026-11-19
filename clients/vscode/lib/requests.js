'use strict';

const vscode = require('vscode');

const { applyRender } = require('./render');
const { answerFor: answerPrompt, promptItems, customChoice } = require('./prompt');
const { answerFor: answerPick, pickItems } = require('./pick');
const { applyEffects } = require('./effects');
const { store } = require('./store');
const { message } = require('./util');

/**
 * Send one canonical key step through `view/key`.
 *
 * @param {string} viewId
 * @param {string} key
 * @param {{cursor: import('../types/protocol').ViewKeyParams.Cursor, selection?: import('../types/protocol').ViewKeyParams.Selection}} info
 */
async function press(viewId, key, info) {
  const state = store.views.get(viewId);
  const client = store.client;
  if (!state || !client) {
    return;
  }

  /** @type {import('../types/protocol').ViewKeyParams.Root} */
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
    const response = /** @type {import('../types/protocol').ViewInvokeResponse.Root} */ (
      await client.sendRequest('view/key', params)
    );
    await applyEffects(response && response.effects, viewId);
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
 * @param {{cursor: import('../types/protocol').ViewInvokeParams.Cursor, selection?: import('../types/protocol').ViewInvokeParams.Selection}} info
 */
async function invoke(viewId, command, args, info) {
  const state = store.views.get(viewId);
  const client = store.client;
  if (!state || !client) {
    return;
  }

  /** @type {import('../types/protocol').ViewInvokeParams.Root} */
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
    const response = /** @type {import('../types/protocol').ViewInvokeResponse.Root} */ (
      await client.sendRequest('view/invoke', params)
    );
    await applyEffects(response && response.effects, viewId);
  } catch (error) {
    store.output.appendLine(`view/invoke ${command} failed: ${message(error)}`);
    vscode.window.showWarningMessage(`magit-lsp: ${message(error)}`);
  }
}

/**
 * Apply a `view/render` notification, then acknowledge the version.
 *
 * @param {import('../types/protocol').ViewRender.Root} params
 */
function applyRenderNotification(params) {
  // `menus` and `views` load through this module's consumers, so resolve their
  // exports after the require cycle has finished initialising.
  const { updateQuickPick } = require('./menus');
  const { isMenuKind, activateView, applyCursor } = require('./views');
  if (!params || !params.viewId) {
    return;
  }

  const state = store.views.get(params.viewId);
  if (!state) {
    store.pendingRenders.set(params.viewId, params);
    return;
  }

  const result = applyRender(state.lines, state.version, params);
  if (result.status === 'resync') {
    void resync(state.id);
    return;
  }

  if (result.status === 'full' || result.status === 'edits') {
    state.lines = result.lines;
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

  if (result.status === 'full' || result.status === 'edits') {
    store.documentsChanged.fire(state.uri);
  }
  if (state.menu || isMenuKind(state.descriptor.kind)) {
    updateQuickPick(state);
  }
  if (state.id === store.activeViewId) {
    activateView(state);
  }
  if (params.cursor) {
    applyCursor(state, params.cursor);
  }
  const client = store.client;
  if (client) {
    /** @type {import('../types/protocol').ViewRenderedParams.Root} */
    const rendered = {
      viewId: state.id,
      version: params.version,
    };
    client.sendNotification('view/rendered', rendered);
    store.semanticChanged?.fire();
  }
}

/**
 * Ask the user a server prompt and answer it with `view/promptRespond`.
 *
 * @param {import('../types/protocol').ViewPromptParams.Root} params `view/prompt` params
 */
async function handlePrompt(params) {
  if (!params || !params.promptId || !store.client) {
    return;
  }

  /** @type {{cancelled: boolean, value?: import('../types/protocol').ViewPromptRespondParams.PromptValue}} */
  let answer = { cancelled: true };
  try {
    answer = await promptFor(params);
  } catch (error) {
    store.output.appendLine(`view/prompt "${params.title}" failed: ${message(error)}`);
  }

  /** @type {import('../types/protocol').ViewPromptRespondParams.Root} */
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
 * @param {import('../types/protocol').ViewPromptParams.Root} params `view/prompt` params
 * @returns {Promise<{cancelled: boolean, value?: import('../types/protocol').ViewPromptRespondParams.PromptValue, custom?: boolean}>}
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
    return answerPrompt(params, value);
  }

  if (params.kind === 'confirm' || params.kind === 'choice') {
    const picked = await vscode.window.showQuickPick(promptItems(params), {
      title: params.title,
      placeHolder: params.detail || params.title,
      ignoreFocusOut: true,
    });
    const answer = answerPrompt(params, picked);
    if (!answer.custom) {
      return answer;
    }
    const text = await vscode.window.showInputBox({
      title: params.title,
      placeHolder: params.placeholder || 'Value',
      ignoreFocusOut: true,
    });
    return customChoice(text);
  }

  if (params.kind === 'multiChoice') {
    const preselected = new Set(params.preselected || []);
    const items = promptItems(params).map((row) => ({
      ...row,
      picked: preselected.has(row.id),
    }));
    const picked = await vscode.window.showQuickPick(items, {
      title: params.title,
      placeHolder: params.detail || params.title,
      canPickMany: true,
      ignoreFocusOut: true,
    });
    return answerPrompt(params, picked);
  }

  // An unknown kind cannot be repaired client-side; cancel it.
  return { cancelled: true };
}

/**
 * Ask the user a server pick and answer it with `view/pickRespond`.
 *
 * @param {import('../types/protocol').ViewPickParams.Root} params `view/pick` params
 */
async function handlePick(params) {
  if (!params || !params.pickId || !store.client) {
    return;
  }

  /** @type {{cancelled: boolean, selected: string[]}} */
  let answer = { cancelled: true, selected: [] };
  try {
    answer = await pickFor(params);
  } catch (error) {
    store.output.appendLine(`view/pick "${params.prompt}" failed: ${message(error)}`);
  }

  try {
    await store.clientReady;
    await store.client.sendRequest('view/pickRespond', {
      pickId: params.pickId,
      cancelled: answer.cancelled,
      selected: answer.selected,
    });
  } catch (error) {
    store.output.appendLine(`view/pickRespond failed: ${message(error)}`);
  }
}

/**
 * Show one pick with the matching VSCode widget.
 *
 * Free text and grouped items are not declared client capabilities, so they
 * warn and cancel rather than presenting a shape the server cannot read.
 *
 * @param {import('../types/protocol').ViewPickParams.Root} params `view/pick` params
 * @returns {Promise<{cancelled: boolean, selected: string[]}>}
 */
async function pickFor(params) {
  const items = params.items || [];
  if (params.allowFreeText || items.some((item) => item.group)) {
    const reason = params.allowFreeText
      ? 'free-text answers are not supported'
      : 'grouped items are not supported';
    store.output.appendLine(`view/pick "${params.prompt}" cancelled: ${reason}`);
    vscode.window.showWarningMessage(`magit-lsp: pick cancelled, ${reason}`);
    return { cancelled: true, selected: [] };
  }

  const picked = await vscode.window.showQuickPick(pickItems(params), {
    title: params.prompt,
    canPickMany: Boolean(params.multi),
    ignoreFocusOut: true,
  });
  return answerPick(params, picked);
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
    const result = /** @type {import('../types/protocol').ViewRender.Root|undefined} */ (
      await client.sendRequest('view/resync', { viewId })
    );
    if (result) {
      applyRenderNotification(result);
    }
  } catch (error) {
    store.output.appendLine(`view/resync failed: ${message(error)}`);
  }
}

exports.press = press;
exports.invoke = invoke;
exports.applyRender = applyRenderNotification;
exports.handlePrompt = handlePrompt;
exports.promptFor = promptFor;
exports.handlePick = handlePick;
