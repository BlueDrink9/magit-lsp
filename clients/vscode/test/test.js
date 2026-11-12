'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const context = require('../lib/context');
const render = require('../lib/render');
const menu = require('../lib/menu');
const foldkeys = require('../lib/foldkeys');
const prompt = require('../lib/prompt');
const pick = require('../lib/pick');
const edit = require('../lib/edit');
const promptVectors = require('../../../tests/data/client-contracts.json').prompts;
const pickVectors = require('../../../tests/data/client-contracts.json').picks;
const noVisionPolicy = require('./no-vision');
const versions = require('../lib/version');
const { createVersionFixture } = require('./e2e/version-fixture');
const { selectSuiteMode, withSuiteMode } = require('./e2e/focused-mode');
const renderVectors = require('../../../tests/data/client-contracts.json').renders;

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

test('optional VSCode runner selects explicit and PATH editors in precedence order', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vscode-editor-selection-'));
  const bin = path.join(directory, 'bin');
  fs.mkdirSync(bin);
  const codium = path.join(bin, 'codium');
  const code = path.join(bin, 'code');
  const explicit = path.join(directory, 'explicit-editor');
  for (const file of [codium, code, explicit]) {
    fs.writeFileSync(file, '');
    fs.chmodSync(file, 0o755);
  }
  const runner = path.join(__dirname, 'e2e', 'run.mjs');
  const select = (extra = {}) => spawnSync(process.execPath, [runner, '--select-editor'], {
    encoding: 'utf8', env: { ...process.env, PATH: bin, MAGIT_VSCODE_BIN: '', ...extra },
  });
  try {
    assert.equal(select({ MAGIT_VSCODE_BIN: explicit }).stdout.trim(), explicit);
    assert.equal(select().stdout.trim(), codium);
    fs.unlinkSync(codium);
    assert.equal(select().stdout.trim(), code);
    fs.unlinkSync(code);
    const absent = select();
    assert.equal(absent.status, 1);
    assert.equal(absent.stdout.trim(), '');
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('focused version runner fails when native prerequisites are missing', () => {
  const runner = path.join(__dirname, 'e2e', 'run.mjs');
  const result = spawnSync(process.execPath, [runner], {
    encoding: 'utf8', env: { PATH: '', DISPLAY: '', MAGIT_VSCODE_BIN: '', MAGIT_VSCODE_VERSION_ONLY: '1' },
  });
  assert.equal(result.status, 1);
  assert.match(result.stdout, /^FAIL e2e: no `code`\/`codium` binary found/m);
});

test('version-only launch selects the version suite branch instead of generic evidence', () => {
  const launchEnvironment = withSuiteMode({ MAGIT_VSCODE_VERSION_ONLY: '1' });
  assert.equal(launchEnvironment.MAGIT_VSCODE_SUITE_MODE, 'version');
  assert.equal(selectSuiteMode(launchEnvironment), 'version');
  assert.notEqual(selectSuiteMode(launchEnvironment), 'generic');
});

test('mismatched extension fixture isolates manifest version, runtime and cleanup', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vscode-version-fixture-'));
  const sourceManifest = fs.readFileSync(path.join(__dirname, '..', 'package.json'));
  const fixture = createVersionFixture(directory);
  try {
    const copiedManifest = JSON.parse(fs.readFileSync(path.join(fixture.extensionPath, 'package.json'), 'utf8'));
    const original = JSON.parse(sourceManifest);
    assert.equal(copiedManifest.version, '9999.0.0');
    assert.equal(copiedManifest.name, original.name);
    assert.equal(copiedManifest.publisher, original.publisher);
    assert.equal(copiedManifest.main, original.main);
    assert.deepEqual(copiedManifest.contributes, original.contributes);
    assert.equal(fs.existsSync(path.join(fixture.extensionPath, 'extension.js')), true);
    assert.equal(fs.existsSync(path.join(fixture.extensionPath, 'lib', 'session.js')), true);
    assert.equal(require.resolve('jsonc-parser', { paths: [fixture.extensionPath] }),
      require.resolve('jsonc-parser', { paths: [path.join(__dirname, '..')] }));
    assert.deepEqual(fs.readFileSync(path.join(__dirname, '..', 'package.json')), sourceManifest);
  } finally {
    fixture.cleanup();
    assert.equal(fs.existsSync(fixture.extensionPath), false);
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

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

for (const vector of renderVectors) {
  test(`render contract: ${vector.name}`, () => {
    const result = render.applyRender(vector.initialLines, vector.initialVersion, vector.render);
    assert.deepEqual(result.lines, vector.expectedLines);
    assert.equal(result.version, vector.expectedVersion);
    assert.equal(result.status, vector.outcome === 'resync' ? 'resync' : vector.render.text ? 'full' : vector.render.edits ? 'edits' : 'none');
    if (vector.expectedMetadata) assert.equal(result.mode, vector.expectedMetadata.mode);
  });
}

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

test('QuickPick rows preserve server activation steps for every canonical shape', () => {
  const cases = [
    ['-a', ['-', 'a']], ['=A', ['=', 'A']], ['g l', ['g', 'l']],
    ['ctrl+t', ['ctrl+t']], ['Enter', ['Enter']], ['G', ['G']], ['Plus', ['Plus']],
  ];
  for (const [key, activationSteps] of cases) {
    const item = { kind: 'action', key, activationSteps, label: 'Planned',
      status: { status: 'planned', roadmapId: 'R-4.1' } };
    const row = menu.quickPickItems({ groups: [{ title: 'Actions', items: [item] }] })[1];
    assert.equal(row.key, key);
    assert.equal(row.activationSteps, activationSteps, 'no client parsing or conversion');
    assert.equal(row.status, 'planned');
    assert.equal(row.roadmapId, 'R-4.1');
  }
  assert.deepEqual(menu.routeTyped('ctrl+t'), ['c', 't', 'r', 'l', 'Plus', 't']);
  assert.deepEqual(menu.routeTyped('g l'), ['g', 'Space', 'l']);
  assert.deepEqual(menu.routeTyped('Enter'), ['E', 'n', 't', 'e', 'r']);
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

test('canonical status Enter, fold and refresh bindings keep the focus gate', () => {
  const generated = require('../package.json').contributes.keybindings;
  const required = [
    ['enter', 'common.visit'],
    ['tab', 'common.toggle-section'],
    ['g', 'common.refresh'],
    ['f5', 'common.refresh'],
  ];
  for (const [key, command] of required) {
    const matches = generated.filter((entry) => entry.key === key
      && entry.args && entry.args.command === command
      && entry.when.includes("magit.scope == 'status'"));
    assert.equal(matches.length, 1, `${key} -> ${command} for status`);
    const terms = matches[0].when.split(' && ');
    for (const term of ['magit.active', 'editorTextFocus', '!quickInputVisible',
      '!suggestWidgetVisible', '!findWidgetVisible', '!renameInputVisible',
      '!magit.textMode']) {
      assert(terms.includes(term), `${key}: missing ${term}`);
    }
  }
});

test('semantic-token types map to theme scopes', () => {
  const contributions = require('../package.json').contributes;
  const scopes = contributions.semanticTokenScopes.find(
    (entry) => entry.language === 'magit'
  );
  assert(scopes, 'package.json contributes no magit semanticTokenScopes');
  // The status legend in src/semantic_tokens.rs, dasherised by lib/semantic.js.
  const statusTypes = [
    'magit-section',
    'magit-count',
    'magit-file',
    'magit-file-modified',
    'magit-file-added',
    'magit-file-deleted',
    'magit-file-renamed',
    'magit-file-untracked',
    'magit-hunk-header',
    'magit-diff-added',
    'magit-diff-removed',
    'magit-message-error',
    'magit-message-warn',
  ];
  for (const type of statusTypes) {
    assert(Array.isArray(scopes.scopes[type]) && scopes.scopes[type].length > 0, type);
  }
  assert.deepEqual(scopes.scopes['magit-message-error'], ['invalid']);
  assert.deepEqual(scopes.scopes['magit-message-warn'], ['markup.warning']);
  for (const type of ['magit-section', 'magit-diff-added', 'magit-diff-removed']) {
    assert(
      contributions.semanticTokenTypes.some((entry) => entry.id === type),
      `${type} is not a declared semantic token type`
    );
  }
  assert.deepEqual(scopes.scopes['magit-diff-added'], ['markup.inserted']);
  assert.deepEqual(scopes.scopes['magit-diff-removed'], ['markup.deleted']);
  assert(
    scopes.scopes['magit-section'].includes('entity.name.section'),
    'section headings have no theme scope'
  );
  assert.equal(
    contributions.configurationDefaults['[magit]']['editor.semanticHighlighting.enabled'],
    true,
    'semantic highlighting is not enabled for the magit language'
  );
});

test('one generic dispatcher carries every contributed action binding', () => {
  const contributions = require('../package.json').contributes;
  const clientCommands = contributions.commands.map((entry) => entry.command).sort();
  assert.deepEqual(clientCommands, [
    'magit.cancelEdit',
    'magit.closeEdit',
    'magit.confirmEdit',
    'magit.invoke',
    'magit.key',
    'magit.mirrorFoldShortcuts',
    'magit.open',
  ]);
  // No contributed VSCode command may be named after a server action; every
  // action binding goes through `magit.invoke` with a canonical server ID.
  for (const perAction of ['magit.refresh', 'magit.toggle-fold', 'magit.show-section',
    'magit.hide-section', 'magit.stage']) {
    assert(!clientCommands.includes(perAction), perAction);
  }
  assert.equal(contributions.keybindings.length > 100, true);
  for (const binding of contributions.keybindings) {
    assert.equal(binding.command, 'magit.invoke', binding.key);
    assert.equal(typeof binding.args?.command, 'string', binding.key);
    assert.match(binding.args.command, /^[a-z][a-z0-9-]*\.[a-z][a-z0-9-]*$/);
    assert(!clientCommands.includes(binding.args.command), binding.key);
  }
});

test('managed draft effects accept only the exact full-sync announcement', () => {
  const effect = { type: 'editDocument', mode: 'realFile', runId: 'run-1',
    uri: 'file:///repo/.git/COMMIT_EDITMSG', version: 1, text: 'exact\r\n', kind: 'commit-message' };
  assert(edit.isEditEffect(effect));
  for (const invalid of [{ ...effect, path: '/tmp/message' }, { ...effect, mode: 'unsupported' },
    { ...effect, uri: 'magit-commit://other/message' }, { ...effect, version: 2 },
    { ...effect, requestId: 4 }]) assert(!edit.isEditEffect(invalid));
  assert.equal(edit.editingContext(effect)['magit.active'], false, 'native typing is not a view keymap');
  assert.equal(edit.draftAction('common.quit'), 'close');
  assert.equal(edit.draftAction('commit-message.history-prev'), undefined);
  assert.equal(edit.draftAction('commit-message.history-next'), undefined);
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

test('JSONC parser preserves comment-like escaped strings and rejects malformed input', () => {
  const parsed = foldkeys.parseJsonc('[{"key":"x\\\"//text","command":"editor.fold","note":"/* text */",},]');
  assert.equal(parsed[0].key, 'x"//text');
  assert.throws(() => foldkeys.parseJsonc('[{"command":]'), /Invalid JSONC.*line/);
});

test('native fold shortcuts map to magit.invoke entries', () => {
  const entries = foldkeys.mapFoldKeys(KEYBINDINGS_JSONC);
  assert.equal(entries.length, 5);

  assert.deepEqual(entries[0], {
    key: 'ctrl+shift+[',
    command: 'magit.invoke',
    args: { command: 'common.hide-section' },
    when: 'magit.active && !magit.textMode && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible',
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

test('shared prompt vectors preserve protocol answer semantics', () => {
  for (const vector of promptVectors) {
    const params = vector.params;
    let answer;
    if (params.kind === 'confirm') {
      const items = prompt.promptItems(params);
      if (vector.focus) {
        assert.deepEqual(items.map((item) => item.label), vector.focus, vector.name);
      }
      answer = prompt.answerFor(params, vector.response === null
        ? undefined : { id: vector.response });
    } else if (params.kind === 'text') {
      answer = prompt.answerFor(params, vector.response);
    } else if (params.kind === 'choice') {
      answer = vector.response === '__custom__'
        ? prompt.customChoice(vector.customResponse)
        : prompt.answerFor(params, vector.response === null
          ? undefined : { id: vector.response });
    } else {
      answer = vector.response === null
        ? { cancelled: true }
        : prompt.answerFor(params, vector.response.map((id) => ({ id })));
    }
    assert.deepEqual(answer, vector.answer, vector.name);
  }
});

// lib/pick.js --------------------------------------------------------------

test('shared pick vectors answer view/pickRespond exactly', () => {
  for (const vector of pickVectors) {
    const params = vector.params;
    const items = pick.pickItems(params);
    assert.deepEqual(items.map((item) => item.label), params.items.map((item) => item.label),
      vector.name);
    if (vector.response === null) {
      assert.deepEqual(pick.answerFor(params, undefined), vector.expected, vector.name);
      continue;
    }
    let picked = items[vector.response - 1];
    if (params.multi) {
      picked = [picked];
    }
    assert.deepEqual(pick.answerFor(params, picked), vector.expected, vector.name);
  }
});

test('pick items carry preselection, description and detail', () => {
  const params = {
    items: [
      { id: 'a', label: 'A', description: 'first', detail: 'detail a' },
      { id: 'b', label: 'B' },
    ],
    preselection: ['b'],
  };
  assert.deepEqual(pick.pickItems(params), [
    { id: 'a', label: 'A', description: 'first', detail: 'detail a', picked: false },
    { id: 'b', label: 'B', description: undefined, detail: undefined, picked: true },
  ]);
});

test('cancelling a pick answers an empty selection', () => {
  assert.deepEqual(pick.answerFor({}, undefined), { cancelled: true, selected: [] });
  assert.deepEqual(pick.answerFor({}, null), { cancelled: true, selected: [] });
});

test('a multi pick maps every picked row to its id', () => {
  assert.deepEqual(
    pick.answerFor({ multi: true }, [{ id: 'a' }, { id: 'c' }]),
    { cancelled: false, selected: ['a', 'c'] }
  );
});

// summary ------------------------------------------------------------------

test('startup version warnings distinguish matching, mismatched and missing metadata', () => {
  assert.equal(versions.warning('1.2.3', '1.2.3', '/bin/magit-lsp'), undefined);
  assert.match(versions.warning('1.2.3', '2.0.0', '/bin/magit-lsp'), /1\.2\.3.*2\.0\.0.*\/bin\/magit-lsp/);
  assert.match(versions.warning('1.2.3', undefined, '/bin/magit-lsp'), /1\.2\.3.*unknown.*\/bin\/magit-lsp/);
});

test('PATH command resolution selects executable and preserves unresolved command', () => {
  const path = require('node:path');
  const executable = path.basename(process.execPath);
  assert.equal(versions.resolveCommand(executable, { PATH: path.dirname(process.execPath) }), process.execPath);
  assert.equal(versions.resolveCommand('missing-tool', { PATH: '' }), 'missing-tool');
});

test('Unix PATH resolution skips non-executables and returns absolute paths', () => {
  const fs = require('node:fs');
  const os = require('node:os');
  const path = require('node:path');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-version-'));
  try {
    const first = path.join(root, 'first');
    const second = path.join(root, 'second');
    fs.mkdirSync(first); fs.mkdirSync(second);
    fs.writeFileSync(path.join(first, 'tool'), '');
    fs.writeFileSync(path.join(second, 'tool'), '');
    fs.chmodSync(path.join(first, 'tool'), 0o010);
    fs.chmodSync(path.join(second, 'tool'), 0o755);
    assert.equal(versions.resolveCommand('tool', { PATH: `${first}:${second}` }, 'linux'), path.join(second, 'tool'));
    assert.equal(versions.resolveCommand('tool', { PATH: path.relative(process.cwd(), second) }, 'linux'), path.join(second, 'tool'));
    assert.equal(versions.resolveCommand(path.join(first, 'tool'), { PATH: '' }, 'linux'), path.join(first, 'tool'));
    assert.equal(versions.resolveCommand('unknown', { PATH: '' }, 'linux'), 'unknown');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('Windows PATH resolution does not duplicate an existing PATHEXT suffix', () => {
  const fs = require('node:fs');
  const os = require('node:os');
  const path = require('node:path');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-version-win-'));
  try {
    fs.writeFileSync(path.join(root, 'node.exe'), '');
    assert.equal(versions.resolveCommand('node.EXE', { PATH: root, PATHEXT: '.EXE;.CMD' }, 'win32'), path.join(root, 'node.exe'));
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('executable VSCode e2e paths enforce no-vision policy', noVisionPolicy);

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
