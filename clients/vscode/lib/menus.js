"use strict";

const vscode = require("vscode");

const { quickPickItems, routeTyped } = require("./menu");
const { press } = require("./requests");
const { store } = require("./store");
const { message } = require("./util");

/**
 * A menu QuickPick row: a VSCode item plus the canonical menu key and steps.
 *
 * @typedef {vscode.QuickPickItem & {
 *   key?: string,
 *   activationSteps?: string[],
 *   itemKind?: string,
 *   status?: string,
 * }} MenuQuickPickItem
 */

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
  // `views` requires this module; resolve its export after the cycle loads.
  const { activateView } = require("./views");
  if (state.quickPick) {
    updateQuickPick(state);
    state.quickPick.show();
    activateView(state);
    return;
  }

  /** @type {vscode.QuickPick<MenuQuickPickItem>} */
  const quickPick = vscode.window.createQuickPick();
  quickPick.title = state.menu ? state.menu.title : state.descriptor.title;
  quickPick.placeholder = "Press a menu key";
  quickPick.matchOnDescription = true;
  quickPick.matchOnDetail = true;
  quickPick.items = menuItems(state.menu);
  state.quickPick = quickPick;
  store.quickPickViewId = state.id;

  quickPick.onDidChangeValue((value) => {
    if (!value) {
      return;
    }
    const steps = routeTyped(value);
    // Clearing the input raises one empty change event, which is ignored.
    quickPick.value = "";
    queueSteps(state, steps);
  });

  quickPick.onDidAccept(() => {
    const picked = quickPick.selectedItems && quickPick.selectedItems[0];
    if (picked && picked.key) {
      // A selectable menu row always carries its activation steps on the wire.
      queueSteps(state, /** @type {string[]} */ (picked.activationSteps));
    }
  });

  quickPick.onDidHide(() => {
    if (state.quickPickHiding) {
      return;
    }
    state.quickPickHiding = true;
    // Escape in the QuickPick never reaches a keybinding; tell the server to
    // pop the menu so the view stack stays in sync.
    queueSteps(state, ["Escape"]);
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
 * @param {import('../types/protocol').Menu.Root|undefined} model
 * @returns {MenuQuickPickItem[]}
 */
function menuItems(model) {
  return quickPickItems(model).map((row) => {
    if (row.kind === "separator") {
      return { label: row.label, kind: vscode.QuickPickItemKind.Separator };
    }
    return {
      label: row.label,
      description: row.description,
      detail: row.roadmapId ? `planned: ${row.roadmapId}` : row.reason,
      kind: vscode.QuickPickItemKind.Default,
      key: row.key,
      activationSteps: row.activationSteps,
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
  if (store.quickPickViewId === state.id) {
    store.quickPickViewId = undefined;
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
        if (!store.views.has(state.id)) {
          return;
        }
        await press(state.id, step, { cursor: { line: 0, column: 0 } });
        if (!store.views.has(state.id)) {
          return;
        }
      }
    })
    .catch((error) => {
      store.output.appendLine(`key failed: ${message(error)}`);
    });
}

exports.showMenu = showMenu;
exports.updateQuickPick = updateQuickPick;
exports.hideMenu = hideMenu;
exports.queueSteps = queueSteps;
