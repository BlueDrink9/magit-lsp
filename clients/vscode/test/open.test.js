'use strict';

const assert = require('node:assert/strict');
const Module = require('node:module');

/** Exercise status opening through the registered VSCode command and provider. */
module.exports = async function testStatusOpenLifecycle() {
  const commands = new Map();
  const providers = new Map();
  const notifications = new Map();
  const documents = [];
  const tabs = [];
  let openCount = 0;
  let renderVersion = 0;
  let closeDocument;
  let fileOpened = false;
  const renderPayloads = [];
  const requests = [];

  class Uri {
    constructor({ scheme, authority = '', path = '' }) {
      Object.assign(this, { scheme, authority, path });
    }
    static from(parts) { return new Uri(parts); }
    static parse(value) {
      const match = /^([^:]+):\/\/([^/]*)(\/.*)$/.exec(value);
      return new Uri({ scheme: match[1], authority: match[2], path: match[3] });
    }
    toString() { return `${this.scheme}://${this.authority}${this.path}`; }
  }
  class EventEmitter {
    constructor() { this.event = () => {}; }
    fire() {}
    dispose() {}
  }
  const vscode = {
    Uri,
    EventEmitter,
    QuickPickItemKind: { Default: 0, Separator: 1 },
    Position: class Position {},
    Selection: class Selection {},
    Range: class Range {},
    TextEditorRevealType: { InCenterIfOutsideViewport: 0 },
    workspace: {
      workspaceFolders: [{ uri: { fsPath: '/repo' } }],
      getConfiguration: () => ({ get: () => undefined }),
      registerTextDocumentContentProvider: (scheme, provider) => {
        providers.set(scheme, provider);
        return { dispose() {} };
      },
      onDidCloseTextDocument: (callback) => { closeDocument = callback; return { dispose() {} }; },
      openTextDocument: async (uri) => {
        if (uri.scheme !== 'magit') {
          fileOpened = true;
        }
        const document = {
          uri,
          getText: () => providers.get(uri.scheme).provideTextDocumentContent(uri),
          lineCount: 1,
          lineAt: () => ({ text: '' }),
        };
        documents.push(document);
        return document;
      },
    },
    window: {
      createOutputChannel: () => ({ appendLine() {}, dispose() {} }),
      onDidChangeActiveTextEditor: () => ({ dispose() {} }),
      showTextDocument: async (document) => {
        tabs.push(document);
        return { document, selection: { isEmpty: true } };
      },
      showErrorMessage: (text) => { throw new Error(text); },
      showWarningMessage: () => {},
      showInformationMessage: () => {},
      visibleTextEditors: [],
      tabGroups: { all: [] },
    },
    SemanticTokensLegend: class {},
    languages: {
      registerDocumentSemanticTokensProvider: () => ({ dispose() {} }),
      setTextDocumentLanguage: async (document) => { await closeDocument(document); },
    },
    commands: {
      registerCommand: (name, callback) => {
        commands.set(name, callback);
        return { dispose() {} };
      },
      executeCommand: async () => {},
    },
  };

  class LanguageClient {
    constructor() {
      this.handlers = notifications;
      this.initializeResult = { capabilities: { semanticTokensProvider: {
        legend: { tokenTypes: ['magit.section'], tokenModifiers: [] },
      } } };
    }
    onNotification(name, handler) { notifications.set(name, handler); }
    start() { return Promise.resolve(); }
    stop() { return Promise.resolve(); }
    async sendRequest(method, params) {
      requests.push({ method, params });
      if (method === 'view/invoke') return {};
      assert.equal(method, 'view/open');
      openCount += 1;
      const descriptor = {
        id: 'status-1', kind: 'status', uri: 'magit://status/repo',
        title: 'Status', mode: 'interactive', scope: 'status', capture: false,
      };
      const payload = {
        viewId: descriptor.id,
        version: ++renderVersion,
        text: [`Status for /repo (${openCount})`, 'Working tree clean'],
        mode: 'interactive',
      };
      renderPayloads.push(payload);
      if (openCount === 1) {
        // Render can precede the descriptor response; it must be replayed.
        notifications.get('view/render')(payload);
        return descriptor;
      }
      // On repeat open the descriptor resolves first and the render follows.
      queueMicrotask(() => notifications.get('view/render')(payload));
      return descriptor;
    }
    sendNotification() {}
  }

  const originalLoad = Module._load;
  Module._load = function load(request, parent, isMain) {
    if (request === 'vscode') return vscode;
    if (request === 'vscode-languageclient/node') return { LanguageClient };
    return originalLoad.call(this, request, parent, isMain);
  };
  let extension;
  try {
    extension = require('../extension');
  } finally {
    Module._load = originalLoad;
  }

  const context = { subscriptions: [] };
  extension.activate(context);
  await commands.get('magit.open')();
  assert.equal(openCount, 1);
  assert.equal(tabs.length, 1);
  assert.match(tabs[0].uri.path, /Status$/);
  const firstText = renderPayloads[0].text.join('\n');
  assert.equal(tabs[0].getText(), firstText);
  assert.equal(tabs[0].getText().trim().length > 0, true);
  assert.notEqual(tabs[0].getText(), 'Status');
  assert.equal(tabs[0].uri.scheme, 'magit');
  assert.equal(fileOpened, false, 'first open must not use a file document');
  assert.equal(documents.every((document) => document.uri.scheme === 'magit'), true);
  const statusUri = tabs[0].uri.toString();

  await commands.get('magit.open')();
  assert.equal(openCount, 1);
  assert.equal(tabs.length, 2);
  assert.equal(tabs[1].uri.toString(), tabs[0].uri.toString());
  assert.equal(tabs[1].getText(), firstText);
  assert.equal(documents.length, 2);
  assert.equal(providers.has('file'), false);

  // Two concurrent opens must share the one existing status view.
  await Promise.all([commands.get('magit.open')(), commands.get('magit.open')()]);
  assert.equal(openCount, 1, 'concurrent opens must reuse the one server view');
  assert.equal(
    requests.filter((request) => request.method === 'view/open').length,
    1,
    'concurrent opens must send one view/open request'
  );
  for (const tab of tabs.slice(2)) {
    assert.equal(tab.uri.toString(), statusUri);
    assert.equal(tab.getText(), firstText);
  }
  assert.equal(fileOpened, false, 'repeat opens must not use a file document');

  // A refresh render must replace the server text on the same read-only URI.
  await commands.get('magit.invoke')({ command: 'common.refresh' });
  assert.equal(requests.at(-1).params.command, 'common.refresh');
  const refreshedVersion = ++renderVersion;
  notifications.get('view/render')({
    viewId: 'status-1',
    version: refreshedVersion,
    baseVersion: refreshedVersion - 1,
    edits: [{ startLine: 1, endLine: 2, lines: ['Working tree changed'] }],
    mode: 'interactive',
  });
  assert.equal(tabs[0].getText(), 'Status for /repo (1)\nWorking tree changed');
  assert.equal(tabs[1].getText(), tabs[0].getText());
  assert.equal(tabs[0].uri.toString(), statusUri);
  assert.equal(fileOpened, false, 'refresh must not use a file document');
  assert.equal(providers.has('file'), false);

  await commands.get('magit.invoke')({ command: 'arbitrary.command', args: { value: 42 } });
  assert.deepEqual(requests.at(-1), {
    method: 'view/invoke', params: {
      viewId: 'status-1', command: 'arbitrary.command', args: { value: 42 },
      version: refreshedVersion,
      cursor: { line: 0, column: 0 },
    },
  });

  // Exercise the registered dispatcher, including absence of nested arguments.
  for (const command of ['common.refresh', 'common.toggle-section']) {
    await commands.get('magit.invoke')({ command });
    assert.deepEqual(requests.at(-1), {
      method: 'view/invoke', params: {
        viewId: 'status-1', command, version: refreshedVersion, cursor: { line: 0, column: 0 },
      },
    });
  }
  // Nested command data must remain opaque, even when it also has an `args` key.
  for (const args of [{ args: { recursive: true } }, false, null, 'value']) {
    await commands.get('magit.invoke')({ command: 'common.show-section', args });
    assert.deepEqual(requests.at(-1).params.args, args);
  }
  const contributions = require('../package.json').contributes;
  for (const { command } of contributions.commands) {
    assert(commands.has(command), `contributed command is registered: ${command}`);
  }
  const bindings = contributions.keybindings;
  for (const [key, command] of [['tab', 'common.toggle-section'], ['f5', 'common.refresh']]) {
    const binding = bindings.find((entry) => entry.key === key && entry.args.command === command);
    assert(binding, `${key} dispatches ${command}`);
    await commands.get(binding.command)(binding.args);
    assert.equal(requests.at(-1).params.command, command);
  }
  // Every generated payload must traverse the actual registered dispatcher,
  // not just match a snapshot. Rust manifest tests check server registry IDs.
  for (const binding of bindings) {
    assert(commands.has(binding.command), binding.command);
    await commands.get(binding.command)(binding.args);
    assert.equal(requests.at(-1).method, 'view/invoke');
    assert.equal(requests.at(-1).params.command, binding.args.command);
    assert.deepEqual(requests.at(-1).params.args, binding.args.args);
  }
  assert(!commands.has('magit.refresh'));
  assert(!commands.has('magit.toggle-fold'));

  await extension.deactivate();
};
