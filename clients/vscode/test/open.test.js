'use strict';

const assert = require('node:assert/strict');
const Module = require('node:module');
const path = require('node:path');

/**
 * Load the extension against one harness's editor and language-client fakes.
 *
 * Evicting the cached extension modules lets each harness bind the fakes it
 * constructs, so one test's client and editor state never leak into another.
 *
 * @param {object} vscode the editor fake
 * @param {Function} LanguageClient the language-client fake
 * @returns {object} the extension module
 */
function loadExtension(vscode, LanguageClient) {
  const root = path.join(__dirname, '..');
  const owned = [path.join(root, 'extension.js'), path.join(root, 'package.json')];
  const prefixes = [path.join(root, 'lib') + path.sep, path.join(root, 'types') + path.sep];
  for (const key of Object.keys(require.cache)) {
    if (owned.includes(key) || prefixes.some((prefix) => key.startsWith(prefix))) {
      delete require.cache[key];
    }
  }
  let extension;
  const originalLoad = Module._load;
  Module._load = function load(request, parent, isMain) {
    if (request === 'vscode') return vscode;
    if (request === 'vscode-languageclient/node') return { LanguageClient };
    return originalLoad.call(this, request, parent, isMain);
  };
  try {
    extension = require('../extension');
  } finally {
    Module._load = originalLoad;
  }
  return extension;
}

/** Exercise status opening through the registered VSCode command and provider. */
async function testStatusOpenLifecycle() {
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
  const warnings = [];
  const output = [];
  let closeFailure = false;
  let quitOnInvoke = false;

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
      onDidChangeTextDocument: () => ({ dispose() {} }),
      registerFileSystemProvider: (scheme, provider) => {
        providers.set(scheme, provider);
        return { dispose() {} };
      },
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
      createOutputChannel: () => ({ appendLine(text) { output.push(text); }, dispose() {} }),
      onDidChangeActiveTextEditor: () => ({ dispose() {} }),
      showTextDocument: async (document) => {
        tabs.push(document);
        return { document, selection: { isEmpty: true } };
      },
      showErrorMessage: (text) => { throw new Error(text); },
      showWarningMessage: (text) => { warnings.push(text); },
      showInformationMessage: () => {},
      visibleTextEditors: [],
      tabGroups: {
        get all() { return [{ tabs: tabs.map((document) => ({ input: { uri: document.uri } })) }]; },
        onDidChangeTabs: () => ({ dispose() {} }),
        async close(closing) {
          for (const tab of closing) {
            const index = tabs.findIndex((document) => document.uri.toString() === tab.input.uri.toString());
            if (index >= 0) tabs.splice(index, 1);
          }
        },
      },
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
    constructor(_id, _name, _server, options) {
      this.handlers = notifications;
      this.features = [];
      this.options = options;
      this.initializeResult = { serverInfo: { version: require('../package.json').version }, capabilities: {
        semanticTokensProvider: {
          legend: { tokenTypes: ['magit.section'], tokenModifiers: [] },
        },
        experimental: { magit: { protocolVersion: 1 } },
      } };
    }
    // anchor: vscode-languageclient 9.0.1 StaticFeature (features.d.ts), production session.js and native VSCodium evidence: docs/evidence/typed-capability-declarations.md; this fake only accepts registration and does not model initialization.
    registerFeature(feature) { this.features.push(feature); }
    onNotification(name, handler) { notifications.set(name, handler); }
    start() { return Promise.resolve(); }
    stop() { return Promise.resolve(); }
    async sendRequest(method, params) {
      requests.push({ method, params });
      if (method === 'view/close') {
        if (closeFailure) throw new Error('disconnected');
        return null;
      }
       if (method === 'view/invoke') return quitOnInvoke && params.command === 'common.quit'
         ? { effects: [{ type: 'quit' }] } : {};
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
    sendNotification(method) { assert.notEqual(method, 'view/close'); }
  }

  const extension = loadExtension(vscode, LanguageClient);

  const context = { subscriptions: [] };
  extension.activate(context);
  const startedClient = require('../lib/store').store.client;
  const declaration = { experimental: {} };
  startedClient.features[0].fillClientCapabilities(declaration);
  assert.equal(declaration.experimental.magit.v, 1, 'client declares rich session version 1');
  assert.equal(declaration.experimental.magit.minimumVersion, 1, 'client requires minimum rich version 1');
  await commands.get('magit.open')();
  assert.equal(warnings.length, 0, 'matching server metadata emits no startup warning');
  assert.equal(openCount, 1);
  assert.notEqual(require('../lib/store').store.semanticChanged, undefined,
    'matching advertisement registers the rich session');
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

  // Drive the actual workspace close callback, not an isolated send helper.
  const { store } = require('../lib/store');
  const closeRequests = () => requests.filter((request) => request.method === 'view/close');
  for (let cycle = 0; cycle < 3; cycle += 1) {
    const document = tabs.at(-1);
    const count = closeRequests().length;
    await closeDocument(document);
    await closeDocument(document);
    assert.equal(closeRequests().length, count + 1, 'one request per user close');
    assert.deepEqual(closeRequests().at(-1).params, { viewId: 'status-1' });
    assert.equal(store.views.size, 0);
    assert.equal(store.uriToView.size, 0);
    await commands.get('magit.open')();
    assert.equal(store.views.size, 1);
  }
  // Server effects remove local state before a later document-close event.
  const serverClosedDocument = tabs.at(-1);
  const beforeServerClose = closeRequests().length;
  notifications.get('view/effects')({ effects: [{ type: 'popView', viewId: 'status-1' }] });
  await closeDocument(serverClosedDocument);
  assert.equal(closeRequests().length, beforeServerClose, 'server close must not echo');

  await commands.get('magit.open')();
  closeFailure = true;
  await closeDocument(tabs.at(-1));
  assert.equal(store.views.size, 0, 'failure must not resurrect local state');
  assert.equal(output.at(-1), 'view/close failed: disconnected');
  assert.equal(warnings.at(-1), 'magit-lsp: disconnected');
  closeFailure = false;
  await commands.get('magit.open')();
  const client = store.client;
  store.client = undefined;
  const beforeDisconnect = closeRequests().length;
  await closeDocument(tabs.at(-1));
  assert.equal(closeRequests().length, beforeDisconnect);
  assert.equal(store.views.size, 0);
  store.client = client;
  await commands.get('magit.open')();
  const beforeQuitClose = closeRequests().length;
  quitOnInvoke = true;
  await commands.get('magit.invoke')({ command: 'common.quit' });
  quitOnInvoke = false;
  assert.equal(store.views.size, 0, 'root quit removes the stored view');
  assert.equal(store.uriToView.size, 0);
  assert.equal(tabs.some((document) => document.uri.toString() === statusUri), false,
    'root quit closes the native virtual-document tab');
  assert.equal(closeRequests().length, beforeQuitClose, 'root quit does not recursively close');
  await commands.get('magit.open')();
  assert.equal(store.views.size, 1, 'status can be reopened after quit');
  await extension.deactivate();
}

/** Resolve once pending promise callbacks and rejection reporting have run. */
function settle() {
  return new Promise((resolve) => setImmediate(resolve));
}

/**
 * Build one admission harness: a minimal editor fake, a client fake and the
 * scripted initialize result or start rejection.
 *
 * @param {{startError?: string, protocolVersion?: unknown, includeProtocol?: boolean}} options
 */
function admissionHarness(options) {
  const commands = new Map();
  const errorMessages = [];
  const warnings = [];
  const output = [];
  const requests = [];
  const notifications = [];
  let semanticRegistrations = 0;
  const vscode = {
    Uri: { from: (parts) => parts, parse: (value) => value },
    EventEmitter: class {
      constructor() { this.event = () => {}; }
      fire() {}
      dispose() {}
    },
    workspace: {
      workspaceFolders: [{ uri: { fsPath: '/repo' } }],
      getConfiguration: () => ({ get: () => undefined }),
      registerTextDocumentContentProvider: () => ({ dispose() {} }),
      onDidCloseTextDocument: () => ({ dispose() {} }),
      onDidChangeTextDocument: () => ({ dispose() {} }),
    },
    window: {
      createOutputChannel: () => ({
        appendLine(text) { output.push(text); },
        dispose() {},
      }),
      onDidChangeActiveTextEditor: () => ({ dispose() {} }),
      showErrorMessage: (text) => { errorMessages.push(text); },
      showWarningMessage: (text) => { warnings.push(text); },
      tabGroups: {
        get all() { return []; },
        onDidChangeTabs: () => ({ dispose() {} }),
      },
    },
    commands: {
      registerCommand: (name, callback) => {
        commands.set(name, callback);
        return { dispose() {} };
      },
      executeCommand: async () => {},
    },
    languages: {
      registerDocumentSemanticTokensProvider: () => {
        semanticRegistrations += 1;
        return { dispose() {} };
      },
    },
    SemanticTokensLegend: class {},
  };
  class LanguageClient {
    constructor() {
      this.features = [];
      this.handlers = new Map();
      this.initializeResult = {
        serverInfo: { version: require('../package.json').version },
        capabilities: {
          semanticTokensProvider: {
            legend: { tokenTypes: [], tokenModifiers: [] },
          },
          experimental: {
            magit: options.includeProtocol === false
              ? {}
              : { protocolVersion: options.protocolVersion },
          },
        },
      };
    }
    registerFeature(feature) { this.features.push(feature); }
    onNotification(name, handler) { this.handlers.set(name, handler); }
    start() {
      return options.startError
        ? Promise.reject(new Error(options.startError))
        : Promise.resolve();
    }
    stop() { return Promise.resolve(); }
    async sendRequest(method, params) {
      requests.push({ method, params });
      return {};
    }
    async sendNotification(method, params) {
      notifications.push({ method, params });
    }
  }
  return {
    vscode, LanguageClient, commands, errorMessages, warnings, output, requests,
    notifications, semanticRegistrations: () => semanticRegistrations,
  };
}

/**
 * Protocol admission: no advertisement variant or initialize rejection may
 * open a rich view, register semantic tokens or send rich traffic, including
 * on repeated open attempts, and every failure must reach the user readably.
 */
async function testProtocolAdmission() {
  const unhandled = [];
  const onUnhandled = (reason) => { unhandled.push(reason); };
  process.on('unhandledRejection', onUnhandled);
  let harness;
  try {
    const variants = [
      { name: 'missing', includeProtocol: false, advertised: 'none' },
      { name: 'string', protocolVersion: '1', advertised: '"1"' },
      { name: 'zero', protocolVersion: 0, advertised: '0' },
      { name: 'negative', protocolVersion: -1, advertised: '-1' },
      { name: 'fraction', protocolVersion: 1.5, advertised: '1.5' },
      { name: 'different', protocolVersion: 2, advertised: '2' },
    ];
    for (const variant of variants) {
      harness = admissionHarness(variant);
      const extension = loadExtension(harness.vscode, harness.LanguageClient);
      extension.activate({ subscriptions: [] });
      await settle();
      assert.equal(harness.semanticRegistrations(), 0, `${variant.name}: no semantic registration`);
      assert.equal(
        harness.errorMessages.some((text) => text.includes('required version 1')
          && text.includes(`server advertised ${variant.advertised}`)),
        true,
        `${variant.name}: readable required/advertised error`
      );
      assert.equal(
        harness.output.some((text) => text.includes('language client failed')),
        true,
        `${variant.name}: startup failure is not swallowed into readiness`
      );
      const { store } = require('../lib/store');
      await harness.commands.get('magit.open')();
      await harness.commands.get('magit.open')();
      assert.equal(harness.requests.length, 0, `${variant.name}: no rich requests on repeated open`);
      assert.equal(harness.notifications.length, 0, `${variant.name}: no rich notifications on repeated open`);
      assert.equal(store.views.size, 0, `${variant.name}: no rich view opened`);
      assert.equal(store.semanticChanged, undefined, `${variant.name}: no semantic invalidation channel`);
      await extension.deactivate();
    }

    const rejection = 'capabilities.experimental.magit unsupported rich protocol version';
    harness = admissionHarness({ startError: rejection });
    const extension = loadExtension(harness.vscode, harness.LanguageClient);
    extension.activate({ subscriptions: [] });
    await settle();
    assert.equal(
      harness.errorMessages.some((text) => text.includes(rejection)),
      true,
      'initialize rejection reaches the user with the server explanation'
    );
    const { store } = require('../lib/store');
    await harness.commands.get('magit.open')();
    await harness.commands.get('magit.open')();
    assert.equal(harness.requests.length, 0, 'rejected start sends no rich requests');
    assert.equal(harness.notifications.length, 0, 'rejected start sends no rich notifications');
    assert.equal(store.views.size, 0, 'rejected start opens no rich view');
    assert.equal(
      harness.errorMessages.filter((text) => text.includes(rejection)).length >= 3,
      true,
      'repeated opens keep reporting the readable rejection'
    );
    await extension.deactivate();
  } finally {
    process.removeListener('unhandledRejection', onUnhandled);
  }
  await settle();
  assert.deepEqual(unhandled, [], 'admission failures never leave an unhandled rejection');
}

module.exports = testStatusOpenLifecycle;
module.exports.protocolAdmission = testProtocolAdmission;
