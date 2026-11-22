"use strict";

const vscode = require("vscode");

const { store } = require("./store");

const REPLAY_COMMANDS = {
  "ctrl+c": "editor.action.clipboardCopyAction",
  "ctrl+x": "editor.action.clipboardCutAction",
  "ctrl+v": "editor.action.clipboardPasteAction",
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
  if (key === "Space") {
    await vscode.commands.executeCommand("default:type", { text: " " });
    return;
  }
  if (typeof key === "string" && Array.from(key).length === 1) {
    await vscode.commands.executeCommand("default:type", { text: key });
    return;
  }
  store.output.appendLine(`passThrough: cannot replay ${key} from a static binding`);
}

/**
 * Show a message effect at its level.
 *
 * @param {Extract<import('../types/protocol').Effect.Root, {type: 'message'}>} effect
 */
function showMessage(effect) {
  const text = effect.hint ? `${effect.text}\n${effect.hint}` : effect.text || "";
  if (effect.detail) {
    store.output.appendLine(effect.detail);
  }
  if (effect.level === "error") {
    vscode.window.showErrorMessage(text);
  } else if (effect.level === "warn") {
    vscode.window.showWarningMessage(text);
  } else {
    vscode.window.showInformationMessage(text);
  }
}

/**
 * Apply the effects a request returned.
 *
 * @param {import('../types/protocol').Effect.Root[]|undefined} effectList
 */
async function applyEffects(effectList, originViewId) {
  // `drafts` and `views` load through this module's consumers, so resolve them
  // after the require cycle has finished initialising.
  const { open } = require("./drafts");
  const { closeRoot, closeView, ensureView, focusView, showView } = require("./views");
  const origin = originViewId && store.views.get(originViewId);
  let rootId = origin && origin.rootId;
  for (const effect of effectList || []) {
    switch (effect.type) {
      case "message":
        showMessage(effect);
        break;
      case "clipboard":
        await vscode.env.clipboard.writeText(effect.text || "");
        vscode.window.showInformationMessage("Copied to clipboard.");
        break;
      case "passThrough":
        await replay(effect.key);
        break;
      case "pushView":
        {
          const state = ensureView(effect.descriptor);
          if (state) state.rootId = rootId;
        }
        await showView(effect.descriptor);
        break;
      case "popView":
        await closeView(effect.viewId, true);
        break;
      case "focusView":
        await focusView(effect.viewId);
        break;
      case "quit":
        await closeRoot(rootId || originViewId);
        break;
      case "visitFile": {
        const document = await vscode.workspace.openTextDocument(vscode.Uri.parse(effect.uri));
        const editor = await vscode.window.showTextDocument(document, {
          preview: false,
          preserveFocus: false,
        });
        const position = new vscode.Position(effect.line || 0, effect.column || 0);
        editor.selection = new vscode.Selection(position, position);
        editor.revealRange(
          new vscode.Range(position, position),
          vscode.TextEditorRevealType.InCenterIfOutsideViewport,
        );
        break;
      }
      case "editDocument":
        await open(effect, rootId || originViewId);
        break;
      default: {
        // The server owns this union; a newer server may send a tag this
        // client does not know, so read it as a boundary value.
        const unknownEffect = /** @type {{type?: string}} */ (effect);
        store.output.appendLine(`unknown effect: ${unknownEffect.type}`);
      }
    }
  }
}

exports.applyEffects = applyEffects;
