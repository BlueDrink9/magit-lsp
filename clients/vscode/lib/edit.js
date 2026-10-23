'use strict';

/**
 * Pure helpers for Git-owned edit documents (§3.10).
 *
 * The extension opens the real file, marks the editing context and forwards
 * confirm/cancel. Nothing here touches the `vscode` module, so the node test
 * suite can exercise it directly.
 */

/** VSCode language ids for each edit kind. */
const LANGUAGES = {
  'commit-message': 'git-commit',
  'merge-message': 'git-commit',
  'tag-message': 'git-commit',
  'rebase-todo': 'git-rebase',
};

/** The canonical key each commit-message profile binds to confirm/cancel. */
const CONFIRM_KEYS = new Set(['ctrl+Enter', 'Z Z']);
const CANCEL_KEYS = new Set(['ctrl+shift+x', 'Z Q']);

/**
 * Whether an effect asks the client to edit a Git-owned file.
 *
 * @param {object|undefined} effect
 * @returns {boolean}
 */
function isEditEffect(effect) {
  return Boolean(effect && effect.type === 'editDocument' && effect.editId && effect.path);
}

/**
 * The VSCode language id for an edit kind.
 *
 * @param {string|undefined} kind
 * @returns {string}
 */
function languageForKind(kind) {
  return LANGUAGES[kind] || 'git-commit';
}

/**
 * The context keys to set while an edit buffer has focus.
 *
 * `magit.scope` is the server's commit-message scope so the generated
 * keybindings (which call `magit.key`) keep firing.
 *
 * @param {object|undefined} effect
 * @returns {{'magit.active': boolean, 'magit.scope': string, 'magit.textMode': boolean, 'magit.capture': boolean, 'magit.editing': boolean}}
 */
function editingContext(effect) {
  const editing = isEditEffect(effect);
  return {
    'magit.active': editing,
    'magit.scope': editing ? 'commit-message' : '',
    'magit.textMode': false,
    'magit.capture': false,
    'magit.editing': editing,
  };
}

/**
 * The context keys to restore when no edit buffer has focus.
 *
 * @returns {{'magit.editing': boolean}}
 */
function inactiveContext() {
  return { 'magit.editing': false };
}

/**
 * The command a canonical key performs in an edit buffer.
 *
 * @param {string|undefined} key
 * @returns {'confirm'|'cancel'|undefined}
 */
function commandForKey(key) {
  if (typeof key !== 'string') {
    return undefined;
  }
  if (CONFIRM_KEYS.has(key)) {
    return 'confirm';
  }
  if (CANCEL_KEYS.has(key)) {
    return 'cancel';
  }
  return undefined;
}

/**
 * The `view/invoke` args for a confirm/cancel answer.
 *
 * @param {string} editId
 * @returns {{editId: string}}
 */
function answerArgs(editId) {
  return { editId };
}

module.exports = {
  answerArgs,
  commandForKey,
  editingContext,
  inactiveContext,
  isEditEffect,
  languageForKind,
};
