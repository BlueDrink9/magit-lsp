'use strict';

const vscode = require('vscode');

const edits = require('./edits');
const views = require('./views');
const { store } = require('./store');

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
  store.output.appendLine(`passThrough: cannot replay ${key} from a static binding`);
}

/**
 * Show a message effect at its level.
 *
 * @param {object} effect
 */
function showMessage(effect) {
  const text = effect.hint ? `${effect.text}\n${effect.hint}` : effect.text || '';
  if (effect.detail) {
    store.output.appendLine(effect.detail);
  }
  if (effect.level === 'error') {
    vscode.window.showErrorMessage(text);
  } else if (effect.level === 'warn') {
    vscode.window.showWarningMessage(text);
  } else {
    vscode.window.showInformationMessage(text);
  }
}

/**
 * Apply the effects a request returned.
 *
 * @param {object[]|undefined} effects
 */
async function applyEffects(effectList, originViewId) {
  const origin = originViewId && store.views.get(originViewId);
  let rootId = origin && origin.rootId;
  for (const effect of effectList || []) {
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
        {
          const state = views.ensureView(effect.descriptor);
          if (state) state.rootId = rootId || (effect.type === 'openView' ? state.id : undefined);
        }
        await views.showView(effect.descriptor);
        break;
      case 'popView':
        await views.closeView(effect.viewId, true);
        break;
      case 'focusView':
        await views.focusView(effect.viewId);
        break;
      case 'quit':
        await views.closeRoot(rootId || originViewId);
        break;
      case 'visitFile': {
        const document = await vscode.workspace.openTextDocument(
          vscode.Uri.parse(effect.uri)
        );
        const editor = await vscode.window.showTextDocument(document, {
          preview: false,
          preserveFocus: false,
        });
        const position = new vscode.Position(effect.line || 0, effect.column || 0);
        editor.selection = new vscode.Selection(position, position);
        editor.revealRange(
          new vscode.Range(position, position),
          vscode.TextEditorRevealType.InCenterIfOutsideViewport
        );
        break;
      }
      case 'editDocument':
        await edits.openEdit(effect);
        break;
      default:
        store.output.appendLine(`unknown effect: ${effect.type}`);
    }
  }
}

exports.applyEffects = applyEffects;
exports.showMessage = showMessage;
exports.replay = replay;
