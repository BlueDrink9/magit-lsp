'use strict';

/**
 * Pure helpers for native commit-message editing (§3.10).
 *
 * The server announces a mutable, server-owned draft and owns its text; the
 * client only provides a writable document and the versioned barriers. Nothing
 * here touches the `vscode` module, so the node test suite can exercise the
 * wire-shape and command rules directly.
 */

/** Native document scheme for commit-message files. */
const SCHEME = 'file';

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
 * @returns {'confirm'|'cancel'|'close'|undefined}
 */
function draftAction(command) {
  switch (command) {
    case 'common.confirm':
      return 'confirm';
    case 'common.cancel':
      return 'cancel';
    case 'common.quit':
      return 'close';
    default:
      return undefined;
  }
}

/**
 * Whether an effect announces a managed commit draft.
 *
 * Accept the real-file effect only, with the initial version and Git template.
 * The server validates the canonical path; no virtual-provider shim is retained.
 *
 * @param {object|undefined} effect
 * @returns {boolean}
 */
function isEditEffect(effect) {
  return Boolean(
    effect
    && effect.type === 'editDocument'
    && effect.mode === 'realFile'
    && typeof effect.runId === 'string'
    && effect.runId.length > 0
    && typeof effect.uri === 'string'
    && effect.uri.startsWith('file:///')
    && effect.uri.endsWith('/COMMIT_EDITMSG')
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
  inactiveContext,
  isEditEffect,
  languageForKind,
};
