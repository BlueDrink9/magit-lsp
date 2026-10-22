'use strict';

/**
 * Remove `//` and `/* ... *\/` comments from a JSONC string.
 *
 * Comment characters inside strings are left alone, and every removed
 * character becomes a space (newlines are kept) so offsets do not move.
 *
 * @param {string} text
 * @returns {string}
 */
function stripJsonComments(text) {
  if (typeof text !== 'string') {
    return '';
  }

  let out = '';
  let inString = false;
  let escaped = false;
  let lineComment = false;
  let blockComment = false;

  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    const next = text[index + 1];

    if (lineComment) {
      if (character === '\n') {
        lineComment = false;
        out += character;
      } else {
        out += ' ';
      }
      continue;
    }

    if (blockComment) {
      if (character === '*' && next === '/') {
        blockComment = false;
        out += '  ';
        index += 1;
      } else {
        out += character === '\n' ? '\n' : ' ';
      }
      continue;
    }

    if (inString) {
      out += character;
      if (escaped) {
        escaped = false;
      } else if (character === '\\') {
        escaped = true;
      } else if (character === '"') {
        inString = false;
      }
      continue;
    }

    if (character === '"') {
      inString = true;
      out += character;
      continue;
    }

    if (character === '/' && next === '/') {
      lineComment = true;
      out += '  ';
      index += 1;
      continue;
    }

    if (character === '/' && next === '*') {
      blockComment = true;
      out += '  ';
      index += 1;
      continue;
    }

    out += character;
  }

  return out;
}

/**
 * Remove trailing commas before `}` or `]`, which JSONC permits.
 *
 * @param {string} text
 * @returns {string}
 */
function stripTrailingCommas(text) {
  let out = '';
  let inString = false;
  let escaped = false;

  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];

    if (inString) {
      out += character;
      if (escaped) {
        escaped = false;
      } else if (character === '\\') {
        escaped = true;
      } else if (character === '"') {
        inString = false;
      }
      continue;
    }

    if (character === '"') {
      inString = true;
      out += character;
      continue;
    }

    if (character === ',') {
      let lookahead = index + 1;
      while (lookahead < text.length && /\s/.test(text[lookahead])) {
        lookahead += 1;
      }
      if (text[lookahead] === '}' || text[lookahead] === ']') {
        continue;
      }
    }

    out += character;
  }

  return out;
}

/**
 * Parse a `keybindings.json` string, accepting comments and trailing commas.
 *
 * @param {string} text
 * @returns {object[]}
 */
function parseJsonc(text) {
  return JSON.parse(stripTrailingCommas(stripJsonComments(text)));
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
      when: 'magit.active',
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
  stripJsonComments,
  stripTrailingCommas,
  parseJsonc,
  mapFoldCommand,
  mapFoldKeys,
};
