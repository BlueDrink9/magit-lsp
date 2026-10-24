'use strict';

const assert = require('node:assert/strict');

const context = require('../lib/context');
const render = require('../lib/render');
const menu = require('../lib/menu');
const foldkeys = require('../lib/foldkeys');
const prompt = require('../lib/prompt');

const failures = [];
let passed = 0;

/**
 * Run one named test and record its failure instead of stopping the suite.
 *
 * @param {string} name
 * @param {() => void} body
 */
function test(name, body) {
  try {
    body();
    passed += 1;
  } catch (error) {
    failures.push({ name, error });
  }
}

// lib/context.js -----------------------------------------------------------

test('a status descriptor derives the status scope', () => {
  const keys = context.deriveContext(
    { id: 'v1', scope: 'status', capture: false },
    'interactive'
  );
  assert.deepEqual(keys, {
    'magit.active': true,
    'magit.scope': 'status',
    'magit.textMode': false,
    'magit.capture': false,
  });
});

test('a menu descriptor uses the shared menu scope and keeps capture', () => {
  const keys = context.deriveContext(
    { id: 'v2', scope: 'menu.commit', capture: true },
    'interactive'
  );
  assert.equal(keys['magit.scope'], 'menu');
  assert.equal(keys['magit.capture'], true);
  assert.equal(keys['magit.textMode'], false);
});

test('text mode turns capture off', () => {
  const keys = context.deriveContext(
    { id: 'v3', scope: 'menu.rebase', capture: true },
    'text'
  );
  assert.equal(keys['magit.scope'], 'menu');
  assert.equal(keys['magit.textMode'], true);
  assert.equal(keys['magit.capture'], false);
});

test('no descriptor means inactive', () => {
  assert.deepEqual(context.deriveContext(undefined, 'interactive'), {
    'magit.active': false,
    'magit.scope': '',
    'magit.textMode': false,
    'magit.capture': false,
  });
  assert.equal(context.inactiveContext()['magit.active'], false);
});

// lib/render.js ------------------------------------------------------------

test('full text replaces the buffer and advances the version', () => {
  const result = render.applyRender(['old'], 4, {
    viewId: 'v1',
    version: 5,
    text: ['a', 'b'],
    mode: 'interactive',
  });
  assert.deepEqual(result.lines, ['a', 'b']);
  assert.equal(result.version, 5);
  assert.equal(result.mode, 'interactive');
  assert.equal(result.status, 'full');
});

test('line edits apply bottom-up when the base version matches', () => {
  const result = render.applyRender(['a', 'b', 'c', 'd'], 7, {
    viewId: 'v1',
    version: 8,
    baseVersion: 7,
    edits: [
      { startLine: 1, endLine: 3, lines: ['B'] },
      { startLine: 3, endLine: 4, lines: ['D', 'E'] },
    ],
    mode: 'interactive',
  });
  assert.deepEqual(result.lines, ['a', 'B', 'D', 'E']);
  assert.equal(result.version, 8);
  assert.equal(result.status, 'edits');
});

test('a stale base version asks for a resync instead of patching', () => {
  const result = render.applyRender(['a', 'b'], 6, {
    viewId: 'v1',
    version: 9,
    baseVersion: 7,
    edits: [{ startLine: 0, endLine: 1, lines: ['stale'] }],
    mode: 'interactive',
  });
  assert.deepEqual(result.lines, ['a', 'b']);
  assert.equal(result.version, 6);
  assert.equal(result.status, 'resync');
});

test('a render with neither text nor edits leaves the buffer alone', () => {
  const result = render.applyRender(['a'], 3, {
    viewId: 'v1',
    version: 4,
    mode: 'interactive',
  });
  assert.deepEqual(result.lines, ['a']);
  assert.equal(result.status, 'none');
});

// lib/menu.js --------------------------------------------------------------

const MENU_MODEL = {
  id: 'commit',
  title: 'Commit',
  groups: [
    {
      title: 'Arguments',
      items: [
        {
          kind: 'switch',
          key: '-a',
          flag: '--all',
          label: 'Stage all modified and deleted files',
          on: true,
          status: { status: 'implemented' },
        },
        {
          kind: 'option',
          key: '=A',
          flag: '--author=',
          label: 'Override the author',
          value: 'Alice',
          reader: 'text',
          status: { status: 'implemented' },
        },
      ],
    },
    {
      title: 'Create',
      items: [
        {
          kind: 'action',
          key: 'c',
          command: 'commit.create',
          label: 'Commit',
          status: { status: 'implemented' },
        },
        {
          kind: 'action',
          key: 'e',
          command: 'commit.extend',
          label: 'Extend HEAD',
          status: { status: 'planned', roadmapId: 'R-4.1' },
        },
      ],
    },
  ],
  savedArgs: { switches: [], options: {} },
};

test('QuickPick rows follow the menu groups', () => {
  const rows = menu.quickPickItems(MENU_MODEL);
  assert.equal(rows.length, 6);

  assert.equal(rows[0].kind, 'separator');
  assert.equal(rows[0].label, 'Arguments');

  assert.equal(rows[1].kind, 'item');
  assert.equal(rows[1].key, '-a');
  assert.equal(rows[1].label, '-a  Stage all modified and deleted files');
  assert.match(rows[1].description, /--all/);
  assert.match(rows[1].description, /on/);
  assert.equal(rows[1].on, true);

  assert.equal(rows[2].key, '=A');
  assert.equal(rows[2].value, 'Alice');
  assert.match(rows[2].description, /--author=/);

  assert.equal(rows[3].label, 'Create');
  assert.equal(rows[4].key, 'c');
  assert.equal(rows[4].status, 'implemented');

  assert.equal(rows[5].key, 'e');
  assert.equal(rows[5].status, 'planned');
  assert.equal(rows[5].roadmapId, 'R-4.1');
  assert.match(rows[5].description, /planned R-4\.1/);
});

test('typed characters become canonical key steps', () => {
  assert.deepEqual(menu.routeTyped('c'), ['c']);
  assert.deepEqual(menu.routeTyped('-a'), ['-', 'a']);
  assert.deepEqual(menu.routeTyped('=A'), ['=', 'A']);
  assert.deepEqual(menu.routeTyped('?'), ['?']);
  assert.deepEqual(menu.routeTyped(' '), ['Space']);
  assert.deepEqual(menu.routeTyped('+'), ['Plus']);
  assert.deepEqual(menu.routeTyped(undefined), []);
});

test('only the newly typed suffix is forwarded when input coalesces', () => {
  assert.equal(menu.typedDelta('', 'c'), 'c');
  assert.equal(menu.typedDelta('c', 'ca'), 'a');
  assert.equal(menu.typedDelta('c', ''), '');
});

test('pop keys are recognised', () => {
  assert.equal(menu.isPopKey('Escape'), true);
  assert.equal(menu.isPopKey('ctrl+c'), true);
  assert.equal(menu.isPopKey('q'), true);
  assert.equal(menu.isPopKey('c'), false);
});

// lib/foldkeys.js ----------------------------------------------------------

test('generated and mirrored bindings require editor focus and exclude widgets', () => {
  const generated = require('../package.json').contributes.keybindings;
  const mirrored = foldkeys.mapFoldKeys([{ key: 'ctrl+k ctrl+l', command: 'editor.toggleFold' }]);
  for (const entry of [...generated, ...mirrored]) {
    const terms = entry.when.split(' && ');
    for (const term of ['magit.active', 'editorTextFocus', '!quickInputVisible',
      '!suggestWidgetVisible', '!findWidgetVisible', '!renameInputVisible']) {
      assert(terms.includes(term), `${entry.key}: missing ${term}`);
    }
    // Test the focus gate independently of scope/text-mode variants.
    const gate = (state) => terms.every((term) => {
      if (term.startsWith('magit.scope') || term.includes('magit.textMode')) return true;
      return term.startsWith('!') ? !state[term.slice(1)] : !!state[term];
    });
    const status = { 'magit.active': true, editorTextFocus: true };
    assert(gate(status));
    assert(!gate({ ...status, editorTextFocus: false }), 'sidebar/terminal/input focus');
    for (const widget of ['quickInputVisible', 'suggestWidgetVisible', 'findWidgetVisible', 'renameInputVisible']) {
      assert(!gate({ ...status, [widget]: true }), widget);
    }
  }
});

const KEYBINDINGS_JSONC = `[
  // Fold the region under the cursor.
  { "key": "ctrl+shift+[", "command": "editor.fold" },
  /* Unfold it again. */
  { "key": "ctrl+shift+]", "command": "editor.unfold", "when": "editorTextFocus" },
  { "key": "ctrl+k ctrl+l", "command": "editor.toggleFold" },
  { "key": "ctrl+k ctrl+0", "command": "editor.foldAll" },
  { "key": "ctrl+shift+p", "command": "workbench.action.showCommands" },
  {
    "key": "ctrl+shift+[",
    "command": "editor.foldRecursively",
    "mac": "cmd+alt+[",
    "args": { "note": "https://example.com // not a comment" }
  },
]`;

test('JSONC comments and trailing commas are stripped', () => {
  const parsed = foldkeys.parseJsonc(KEYBINDINGS_JSONC);
  assert.equal(parsed.length, 6);
  assert.equal(parsed[5].command, 'editor.foldRecursively');
});

test('native fold shortcuts map to magit.invoke entries', () => {
  const entries = foldkeys.mapFoldKeys(KEYBINDINGS_JSONC);
  assert.equal(entries.length, 5);

  assert.deepEqual(entries[0], {
    key: 'ctrl+shift+[',
    command: 'magit.invoke',
    args: { command: 'common.hide-section' },
    when: 'magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible',
  });
  assert.equal(entries[1].args.command, 'common.show-section');
  assert.equal(entries[2].args.command, 'common.toggle-section');
  assert.equal(entries[3].args.command, 'common.hide-all');

  // The workbench command is not a fold shortcut: order skips it.
  assert.equal(entries[4].key, 'ctrl+shift+[');
  assert.deepEqual(entries[4].args, {
    command: 'common.hide-section',
    args: { recursive: true },
  });
  assert.equal(entries[4].mac, 'cmd+alt+[');
});

test('fold levels become detail changes', () => {
  const entries = foldkeys.mapFoldKeys([
    { key: 'ctrl+k ctrl+1', command: 'editor.foldLevel1' },
  ]);
  assert.equal(entries.length, 1);
  assert.equal(entries[0].args.command, 'common.less-detail');
});

// lib/prompt.js ------------------------------------------------------------

test('confirm rows use the labels and put No first when destructive', () => {
  assert.deepEqual(prompt.promptItems({ kind: 'confirm', title: 'Delete?' }), [
    { id: 'yes', label: 'Yes' },
    { id: 'no', label: 'No' },
  ]);
  assert.deepEqual(
    prompt.promptItems({
      kind: 'confirm',
      title: 'Delete?',
      destructive: true,
      yesLabel: 'Delete',
      noLabel: 'Keep',
    }),
    [
      { id: 'no', label: 'Keep' },
      { id: 'yes', label: 'Delete' },
    ]
  );
});

test('choice rows keep item metadata and add a custom row only when allowed', () => {
  const params = {
    kind: 'choice',
    title: 'Style',
    items: [{ id: 'rebase', label: 'Rebase', description: 'rewrite', detail: 'more' }],
  };
  assert.deepEqual(prompt.promptItems(params), [
    { id: 'rebase', label: 'Rebase', description: 'rewrite', detail: 'more' },
  ]);
  assert.deepEqual(prompt.promptItems({ ...params, allowCustom: true }), [
    { id: 'rebase', label: 'Rebase', description: 'rewrite', detail: 'more' },
    { id: prompt.CUSTOM_ID, label: 'Enter custom value…' },
  ]);
  assert.deepEqual(prompt.promptItems(undefined), []);
});

test('answers use the shape the server expects', () => {
  assert.deepEqual(prompt.answerFor({ kind: 'confirm' }, { id: 'yes' }), {
    cancelled: false,
    value: true,
  });
  assert.deepEqual(prompt.answerFor({ kind: 'confirm' }, { id: 'no' }), {
    cancelled: false,
    value: false,
  });
  assert.deepEqual(prompt.answerFor({ kind: 'text' }, 'Alice'), {
    cancelled: false,
    value: 'Alice',
  });
  assert.deepEqual(prompt.answerFor({ kind: 'choice' }, { id: 'rebase' }), {
    cancelled: false,
    value: { id: 'rebase' },
  });
  assert.deepEqual(prompt.answerFor({ kind: 'choice', allowCustom: true }, { id: prompt.CUSTOM_ID }), {
    cancelled: false,
    custom: true,
  });
  assert.deepEqual(prompt.answerFor({ kind: 'multiChoice' }, [{ id: 'a' }, { id: 'b' }]), {
    cancelled: false,
    value: { ids: ['a', 'b'] },
  });
  assert.deepEqual(prompt.answerFor({ kind: 'text' }, undefined), { cancelled: true });
});

test('a cancelled custom choice cancels the whole prompt', () => {
  assert.deepEqual(prompt.customChoice('main'), {
    cancelled: false,
    value: { custom: 'main' },
  });
  assert.deepEqual(prompt.customChoice(undefined), { cancelled: true });
});

// summary ------------------------------------------------------------------

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`FAIL: ${failure.name}`);
    console.error(failure.error && failure.error.stack ? failure.error.stack : failure.error);
  }
  console.error(`\n${failures.length} failed, ${passed} passed`);
  process.exit(1);
}

require('./open.test')().then(() => {
  console.log(`ok ${passed} tests`);
}).catch((error) => {
  console.error(error && error.stack ? error.stack : error);
  process.exitCode = 1;
});
