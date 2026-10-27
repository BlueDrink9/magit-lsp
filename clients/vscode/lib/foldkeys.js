'use strict';

const jsonc = require('jsonc-parser');

/**
/**
 * Parse a `keybindings.json` string, accepting comments and trailing commas.
 *
 * @param {string} text
 * @returns {object[]}
 */
function parseJsonc(text) {
  const errors = [];
  const value = jsonc.parse(text, errors, { allowTrailingComma: true, disallowComments: false });
  if (errors.length) {
    const error = errors[0];
    const line = text.slice(0, error.offset).split('\n').length;
    throw new Error(`Invalid JSONC at line ${line}, offset ${error.offset}: ${jsonc.printParseErrorCode(error.error)}`);
  }
  return value;
}

/**
 * The equivalent magit command for a VSCode fold command, if there is one.
 *
 * @param {string} command
 * @returns {{command: string, args?: object}|undefined}
 */
function mapFoldCommand(command) {
  if (typeof command !== 'string') {
    return undefined;
  }
  if (FOLD_COMMANDS[command]) {
    return FOLD_COMMANDS[command];
  }
  if (command === 'editor.toggleFold') {
    return { command: 'common.toggle-section' };
  }
  if (command.startsWith('editor.unfold')) {
    return { command: 'common.show-section' };
  }
  if (command.startsWith('editor.fold')) {
    return { command: 'common.hide-section' };
  }
  return undefined;
}

/**
 * Map the user's native fold shortcuts to `magit.invoke` entries.
 *
 * The result can be pasted into `keybindings.json`; `when` is scoped to a
 * magit view so the entries never affect other editors.
 *
 * @param {string|object[]} input JSONC text or an already parsed keybinding list
 * @returns {object[]}
 */
function mapFoldKeys(input) {
  const entries = typeof input === 'string' ? parseJsonc(input) : input;
  if (!Array.isArray(entries)) {
    return [];
  }

  const mapped = [];
  for (const entry of entries) {
    if (!entry || typeof entry !== 'object') {
      continue;
    }
    const target = mapFoldCommand(entry.command);
    if (!target) {
      continue;
    }

    const result = {
      key: entry.key,
      command: 'magit.invoke',
      args: { command: target.command },
      when: 'magit.active && !magit.textMode && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible',
    };
    if (target.args) {
      result.args.args = target.args;
    }
    // Keep platform-specific key overrides as VSCode's own schema expects.
    for (const platform of ['mac', 'linux', 'win']) {
      if (typeof entry[platform] === 'string') {
        result[platform] = entry[platform];
      }
    }
    mapped.push(result);
  }
  return mapped;
}

const FOLD_COMMANDS = {
  'editor.fold': { command: 'common.hide-section' },
  'editor.foldRecursively': { command: 'common.hide-section', args: { recursive: true } },
  'editor.foldAll': { command: 'common.hide-all' },
  'editor.foldAllBlockComment': { command: 'common.hide-all' },
  'editor.foldAllMarkerRegions': { command: 'common.hide-all' },
  'editor.foldAllExcept': { command: 'common.hide-all' },
  'editor.unfold': { command: 'common.show-section' },
  'editor.unfoldRecursively': { command: 'common.show-section', args: { recursive: true } },
  'editor.unfoldAll': { command: 'common.show-all' },
  'editor.unfoldAllMarkerRegions': { command: 'common.show-all' },
  'editor.unfoldAllExcept': { command: 'common.show-all' },
  'editor.toggleFold': { command: 'common.toggle-section' },
};

for (let level = 1; level <= 7; level += 1) {
  FOLD_COMMANDS[`editor.foldLevel${level}`] = { command: 'common.less-detail' };
}

module.exports = {
  parseJsonc,
  mapFoldCommand,
  mapFoldKeys,
};
