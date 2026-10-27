'use strict';

/**
 * Pure helpers for managed commit drafts (§3.10).
 *
 * The server announces a mutable, server-owned draft and owns its text; the
 * client only provides a writable document and the versioned barriers. Nothing
 * here touches the `vscode` module, so the node test suite can exercise the
 * wire-shape and command rules directly.
 */

/** Virtual-document scheme for managed commit drafts. */
const SCHEME = 'magit-commit';

/** The only editor-bridge kind the server announces today. */
const KIND = 'commit-message';

/** VSCode language ids for each managed-draft kind. */
const LANGUAGES = {
  'commit-message': 'git-commit',
};

/**
 * The draft action a generated command ID names.
 *
 * `undefined` is a command the draft does not own; the client must not forward
 * it to the status view while the draft has focus.
 *
 * @param {string|undefined} command
 * @returns {'confirm'|'cancel'|'close'|'history-prev'|'history-next'|undefined}
 */
function draftAction(command) {
  switch (command) {
    case 'common.confirm':
      return 'confirm';
    case 'common.cancel':
      return 'cancel';
    case 'common.quit':
      return 'close';
    case 'commit-message.history-prev':
      return 'history-prev';
    case 'commit-message.history-next':
      return 'history-next';
    default:
      return undefined;
  }
}

/**
 * The history command a draft action names, if any.
 *
 * @param {'history-prev'|'history-next'|undefined} action
 * @returns {string|undefined}
 */
function historyCommand(action) {
  if (action === 'history-prev') {
    return 'commit-message.history-prev';
  }
  if (action === 'history-next') {
    return 'commit-message.history-next';
  }
  return undefined;
}

/**
 * Whether an effect announces a managed commit draft.
 *
 * Only the exact §3.10 shape is accepted: a `magit-commit:` URI, the initial
 * version, the commit-message kind and the exact template. There is no path or
 * real-file mode, and no compatibility shim for the replaced adapter.
 *
 * @param {object|undefined} effect
 * @returns {boolean}
 */
function isEditEffect(effect) {
  return Boolean(
    effect
    && effect.type === 'editDocument'
    && effect.mode === 'managedDraft'
    && typeof effect.runId === 'string'
    && effect.runId.length > 0
    && typeof effect.uri === 'string'
    && effect.uri === `${SCHEME}://${effect.runId}/message`
    && typeof effect.text === 'string'
    && effect.version === 1
    && effect.kind === KIND
    && Object.keys(effect).every((key) => [
      'type', 'mode', 'runId', 'uri', 'text', 'version', 'kind', 'title', 'scope',
    ].includes(key))
  );
}

/**
 * The VSCode language id for a managed-draft kind.
 *
 * @param {string|undefined} kind
 * @returns {string}
 */
function languageForKind(kind) {
  return LANGUAGES[kind] || 'git-commit';
}

/**
 * The context keys to set while a managed draft has focus.
 *
 * Drafts are native text editors, not interactive server views. Disable the
 * static view bindings (including printable q/g/Tab); draft actions are exposed
 * as native commands and editor-title buttons, not captured typing.
 *
 * @param {object|undefined} effect
 * @returns {{'magit.active': boolean, 'magit.scope': string, 'magit.textMode': boolean, 'magit.capture': boolean, 'magit.editing': boolean}}
 */
function editingContext(effect) {
  return {
    'magit.active': false,
    'magit.scope': '',
    'magit.textMode': false,
    'magit.capture': false,
    'magit.editing': Boolean(effect),
  };
}

/**
 * The context keys to restore when no managed draft has focus.
 *
 * @returns {{'magit.editing': boolean}}
 */
function inactiveContext() {
  return { 'magit.editing': false };
}

module.exports = {
  KIND,
  SCHEME,
  draftAction,
  editingContext,
  historyCommand,
  inactiveContext,
  isEditEffect,
  languageForKind,
};
