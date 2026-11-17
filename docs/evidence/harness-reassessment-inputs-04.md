### Client session

### `clients/vscode/lib/session.js` lines 1-147 of 147
```text
   1 | 'use strict';
   2 |
   3 | const vscode = require('vscode');
   4 | const { LanguageClient } = require('vscode-languageclient/node');
   5 |
   6 | const commands = require('./commands');
   7 | const edits = require('./edits');
   8 | const drafts = require('./drafts');
   9 | const effects = require('./effects');
  10 | const requests = require('./requests');
  11 | const semantic = require('./semantic');
  12 | const views = require('./views');
  13 | const { SCHEME, store } = require('./store');
  14 | const { message } = require('./util');
  15 |
  16 | /**
  17 |  * Create the LSP client with the static key-delivery capabilities.
  18 |  *
  19 |  * @returns {LanguageClient}
  20 |  */
  21 | function createClient() {
  22 |   const config = vscode.workspace.getConfiguration('magit-lsp');
  23 |   const command = config.get('serverPath') || 'magit-lsp';
  24 |   const profile = config.get('profile') || 'standard';
  25 |
  26 |   return new LanguageClient(
  27 |     'magit-lsp',
  28 |     'magit-lsp',
  29 |     { command, args: [] },
  30 |     {
  31 |       documentSelector: [{ scheme: SCHEME }],
  32 |       initializationOptions: { profile },
  33 |       uriConverters: {
  34 |         code2Protocol: (uri) => views.stateForUri(uri)?.descriptor.uri || uri.toString(),
  35 |         protocol2Code: (uri) => Array.from(store.views.values())
  36 |           .find((state) => state.descriptor.uri === uri)?.uri || vscode.Uri.parse(uri),
  37 |       },
  38 |       middleware: {
  39 |         // The explicit provider maps dotted server types to legal VSCode IDs.
  40 |         provideDocumentSemanticTokens: () => null,
  41 |         initialize(params, second, third) {
  42 |           params.clientInfo = { name: 'Visual Studio Code', version: vscode.version };
  43 |           params.capabilities = params.capabilities || {};
  44 |           params.capabilities.experimental = params.capabilities.experimental || {};
  45 |           params.capabilities.experimental.magit = {
  46 |             v: 1,
  47 |             keys: 'static',
  48 |             modifiers: 'full',
  49 |             render: 'edits',
  50 |             layout: ['main', 'side', 'bottom', 'float'],
  51 |             effects: ['visitFile', 'editDocument', 'clipboard'],
  52 |             prompts: ['confirm', 'text', 'choice', 'multiChoice', 'secret'],
  53 |             folds: 'server',
  54 |           };
  55 |           const next = typeof second === 'function' ? second : third;
  56 |           return next(params);
  57 |         },
  58 |       },
  59 |     }
  60 |   );
  61 | }
  62 |
  63 | /**
  64 |  * Activate the extension.
  65 |  *
  66 |  * @param {vscode.ExtensionContext} context
  67 |  */
  68 | function activate(context) {
  69 |   store.output = vscode.window.createOutputChannel('magit-lsp');
  70 |   context.subscriptions.push(store.output);
  71 |
  72 |   store.documentsChanged = new vscode.EventEmitter();
  73 |   context.subscriptions.push(store.documentsChanged);
  74 |
  75 |   context.subscriptions.push(
  76 |     vscode.workspace.registerTextDocumentContentProvider(SCHEME, {
  77 |       onDidChange: store.documentsChanged.event,
  78 |       provideTextDocumentContent(uri) {
  79 |         const state = views.stateForUri(uri);
  80 |         return state ? state.lines.join('\n') : '';
  81 |       },
  82 |     })
  83 |   );
  84 |
  85 |   context.subscriptions.push(
  86 |     vscode.commands.registerCommand('magit.open', commands.openStatus),
  87 |     vscode.commands.registerCommand('magit.key', commands.pressCommand),
  88 |      vscode.commands.registerCommand('magit.invoke', async (args) => {
  89 |        if (drafts.current()) return drafts.handleInvoke(args);
  90 |        return commands.invokeCommand(args);
  91 |      }),
  92 |     vscode.commands.registerCommand('magit.confirmEdit', edits.confirmEditCommand),
  93 |      vscode.commands.registerCommand('magit.cancelEdit', edits.cancelEditCommand),
  94 |      vscode.commands.registerCommand('magit.closeEdit', () => {
  95 |        const draft = drafts.current();
  96 |        if (draft) return drafts.close(draft);
  97 |      }),
  98 |     vscode.commands.registerCommand('magit.mirrorFoldShortcuts', commands.mirrorFoldShortcuts),
  99 |      vscode.workspace.onDidCloseTextDocument(edits.onDocumentClosed),
 100 |      vscode.workspace.onDidChangeTextDocument(drafts.onChanged),
 101 |     vscode.window.onDidChangeActiveTextEditor(edits.onActiveEditorChanged)
 102 |    );
 103 |
 104 |    drafts.setup(context);
 105 |
 106 |   store.client = createClient();
 107 |   const client = store.client;
 108 |   client.onNotification('view/render', requests.applyRender);
 109 |    client.onNotification('view/prompt', requests.handlePrompt);
 110 |    // The explicit full-sync stream excludes the draft scheme from the language
 111 |    // client's automatic document selector, avoiding duplicate native versions.
 112 |    if (client.onDidChangeState) context.subscriptions.push(client.onDidChangeState((event) => {
 113 |      if (event.newState === 1) drafts.disconnect('the server disconnected; copy your text before reopening');
 114 |    }));
 115 |   client.onNotification('view/effects', (params) => {
 116 |     void effects.applyEffects(params && params.effects, store.activeViewId);
 117 |   });
 118 |   store.clientReady = client.start().then(() => {
 119 |     store.semanticChanged = semantic.register(
 120 |       vscode,
 121 |       client,
 122 |       views.stateForUri,
 123 |       context.subscriptions
 124 |     );
 125 |   }).catch((error) => {
 126 |     store.output.appendLine(`language client failed: ${message(error)}`);
 127 |     vscode.window.showErrorMessage(`magit-lsp: ${message(error)}`);
 128 |   });
 129 |
 130 |   views.activateView(undefined);
 131 | }
 132 |
 133 | /**
 134 |  * Stop the language client when the extension deactivates.
 135 |  */
 136 | async function deactivate() {
 137 |   store.disposing = true;
 138 |   drafts.disconnect('the editor session is closing; copy your text before reopening');
 139 |   if (store.client) {
 140 |     await store.client.stop();
 141 |     store.client = undefined;
 142 |   }
 143 | }
 144 |
 145 | exports.activate = activate;
 146 | exports.deactivate = deactivate;
 147 | exports.createClient = createClient;
```
### Fake LanguageClient implementation — full source

### `clients/vscode/test/fake/languageclient.js` lines 1-234 of 234
```text
   1 | 'use strict';
   2 |
   3 | /**
   4 |  * A minimal `LanguageClient` replacement that speaks LSP over stdio.
   5 |  *
   6 |  * The node integration test needs the extension to run against the real
   7 |  * `magit-lsp` binary without VSCode or `vscode-languageclient`'s editor
   8 |  * services. This client implements the JSON-RPC framing, the initialize
   9 |  * handshake and the request/notification surface the extension uses. It
  10 |  * records every outgoing message so tests can assert what the client sent.
  11 |  */
  12 |
  13 | const { spawn } = require('node:child_process');
  14 |
  15 | /** Every client this process constructed, for tests to inspect. */
  16 | const instances = [];
  17 |
  18 | /** One outgoing message, recorded for assertions. */
  19 | class Recording {
  20 |   constructor(method, params) {
  21 |     this.method = method;
  22 |     this.params = params;
  23 |   }
  24 | }
  25 |
  26 | /** A JSON-RPC language client over a child process's stdio. */
  27 | class LanguageClient {
  28 |   /**
  29 |    * @param {string} id
  30 |    * @param {string} name
  31 |    * @param {{command: string, args?: string[]}} serverOptions
  32 |    * @param {object} clientOptions
  33 |    */
  34 |   constructor(id, name, serverOptions, clientOptions) {
  35 |     this.id = id;
  36 |     this.name = name;
  37 |     this.serverOptions = serverOptions;
  38 |     this.clientOptions = clientOptions;
  39 |     this.notifications = new Map();
  40 |     this.pending = new Map();
  41 |     this.nextId = 1;
  42 |     this.buffer = Buffer.alloc(0);
  43 |     this.requests = [];
  44 |     this.notes = [];
  45 |     this.initializeResult = undefined;
  46 |     this.stderr = '';
  47 |     this.exited = undefined;
  48 |     instances.push(this);
  49 |   }
  50 |
  51 |   /**
  52 |    * Register a handler for a server notification.
  53 |    *
  54 |    * @param {string} method
  55 |    * @param {(params: unknown) => void} handler
  56 |    */
  57 |   onNotification(method, handler) {
  58 |     this.notifications.set(method, handler);
  59 |   }
  60 |
  61 |   /**
  62 |    * Start the server, run the LSP initialize handshake and resolve.
  63 |    *
  64 |    * @returns {Promise<void>}
  65 |    */
  66 |   async start() {
  67 |     this.process = spawn(this.serverOptions.command, this.serverOptions.args || [], {
  68 |       stdio: ['pipe', 'pipe', 'pipe'],
  69 |     });
  70 |     this.exited = new Promise((resolve) => {
  71 |       this.process.on('exit', (code, signal) => {
  72 |         for (const { reject } of this.pending.values()) {
  73 |           reject(new Error(`magit-lsp exited (${code ?? signal})`));
  74 |         }
  75 |         this.pending.clear();
  76 |         resolve({ code, signal });
  77 |       });
  78 |     });
  79 |     this.process.stdout.on('data', (chunk) => this.receive(chunk));
  80 |     this.process.stderr.on('data', (chunk) => { this.stderr += chunk.toString(); });
  81 |
  82 |     const params = {
  83 |       processId: process.pid,
  84 |       clientInfo: { name: 'magit-lsp-node-test', version: '0.0.0' },
  85 |       rootUri: this.clientOptions.rootUri || null,
  86 |       capabilities: {},
  87 |       initializationOptions: this.clientOptions.initializationOptions,
  88 |     };
  89 |     const next = async (requestParams) => this.request('initialize', requestParams);
  90 |     const middleware = this.clientOptions.middleware || {};
  91 |     if (middleware.initialize) {
  92 |       this.initializeResult = await middleware.initialize(params, next, undefined);
  93 |     } else {
  94 |       this.initializeResult = await next(params);
  95 |     }
  96 |     this.sendNotification('initialized', {});
  97 |   }
  98 |
  99 |   /**
 100 |    * Send one request and resolve with its result.
 101 |    *
 102 |    * @param {string} method
 103 |    * @param {unknown} params
 104 |    * @returns {Promise<any>}
 105 |    */
 106 |   sendRequest(method, params) {
 107 |     return this.request(method, params);
 108 |   }
 109 |
 110 |   /**
 111 |    * Send one request, recording it for assertions first.
 112 |    *
 113 |    * @param {string} method
 114 |    * @param {unknown} params
 115 |    * @returns {Promise<any>}
 116 |    */
 117 |   request(method, params) {
 118 |     const id = this.nextId++;
 119 |     this.requests.push(new Recording(method, params));
 120 |     return new Promise((resolve, reject) => {
 121 |       this.pending.set(id, { resolve, reject });
 122 |       this.write({ jsonrpc: '2.0', id, method, params });
 123 |     });
 124 |   }
 125 |
 126 |   /**
 127 |    * Send one notification and record it.
 128 |    *
 129 |    * @param {string} method
 130 |    * @param {unknown} params
 131 |    */
 132 |   sendNotification(method, params) {
 133 |     this.notes.push(new Recording(method, params));
 134 |     this.write({ jsonrpc: '2.0', method, params });
 135 |   }
 136 |
 137 |   /**
 138 |    * Stop the server cleanly, then force it down if it lingers.
 139 |    *
 140 |    * @returns {Promise<void>}
 141 |    */
 142 |   async stop() {
 143 |     if (!this.process || this.process.exitCode !== null) {
 144 |       return;
 145 |     }
 146 |     try {
 147 |       await this.request('shutdown', null);
 148 |       this.sendNotification('exit', null);
 149 |     } catch {
 150 |       // The server may already be gone; the kill below is the fallback.
 151 |     }
 152 |     const timedOut = await Promise.race([
 153 |       this.exited.then(() => false),
 154 |       new Promise((resolve) => setTimeout(() => resolve(true), 5000)),
 155 |     ]);
 156 |     if (timedOut) {
 157 |       this.process.kill('SIGKILL');
 158 |       await this.exited;
 159 |     }
 160 |   }
 161 |
 162 |   /**
 163 |    * Frame and write one JSON-RPC message.
 164 |    *
 165 |    * @param {object} message
 166 |    */
 167 |   write(message) {
 168 |     const body = Buffer.from(JSON.stringify(message), 'utf8');
 169 |     this.process.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
 170 |     this.process.stdin.write(body);
 171 |   }
 172 |
 173 |   /**
 174 |    * Consume an arbitrary stdout chunk, keeping incomplete frames buffered.
 175 |    *
 176 |    * @param {Buffer} chunk
 177 |    */
 178 |   receive(chunk) {
 179 |     this.buffer = Buffer.concat([this.buffer, chunk]);
 180 |     for (;;) {
 181 |       const headerEnd = this.buffer.indexOf('\r\n\r\n');
 182 |       if (headerEnd < 0) {
 183 |         return;
 184 |       }
 185 |       const header = this.buffer.subarray(0, headerEnd).toString('ascii');
 186 |       const match = /Content-Length:\s*(\d+)/i.exec(header);
 187 |       if (!match) {
 188 |         throw new Error(`malformed LSP header: ${header}`);
 189 |       }
 190 |       const length = Number(match[1]);
 191 |       const start = headerEnd + 4;
 192 |       if (this.buffer.length < start + length) {
 193 |         return;
 194 |       }
 195 |       const body = this.buffer.subarray(start, start + length).toString('utf8');
 196 |       this.buffer = this.buffer.subarray(start + length);
 197 |       this.dispatch(JSON.parse(body));
 198 |     }
 199 |   }
 200 |
 201 |   /**
 202 |    * Route one decoded message to a pending request or a notification handler.
 203 |    *
 204 |    * @param {object} message
 205 |    */
 206 |   dispatch(message) {
 207 |     if (message.id !== undefined && message.method === undefined) {
 208 |       const pending = this.pending.get(message.id);
 209 |       if (!pending) {
 210 |         return;
 211 |       }
 212 |       this.pending.delete(message.id);
 213 |       if (message.error) {
 214 |         pending.reject(new Error(message.error.message || JSON.stringify(message.error)));
 215 |       } else {
 216 |         pending.resolve(message.result);
 217 |       }
 218 |       return;
 219 |     }
 220 |     if (message.method !== undefined && message.id !== undefined) {
 221 |       // Server-originated requests are not part of this protocol; answer null.
 222 |       this.write({ jsonrpc: '2.0', id: message.id, result: null });
 223 |       return;
 224 |     }
 225 |     if (message.method !== undefined) {
 226 |       const handler = this.notifications.get(message.method);
 227 |       if (handler) {
 228 |         handler(message.params);
 229 |       }
 230 |     }
 231 |   }
 232 | }
 233 |
 234 | module.exports = { LanguageClient, instances };
```
### Fake VSCode implementation and implemented API — full source

### `clients/vscode/test/fake/vscode.js` lines 1-588 of 588
```text
   1 | 'use strict';
   2 |
   3 | /**
   4 |  * The scripted `vscode` module used by the node integration test.
   5 |  *
   6 |  * It implements only the API surface `lib/` and `extension.js` call, records
   7 |  * every widget interaction for assertions and lets the test script answers to
   8 |  * QuickPick and input prompts. Nothing here parses server text or structure.
   9 |  */
  10 |
  11 | const fs = require('node:fs');
  12 |
  13 | /** All observable fake-window state, reset between test cases. */
  14 | const state = {
  15 |   commands: new Map(),
  16 |   providers: new Map(),
  17 |   fileProviders: new Map(),
  18 |   documents: new Map(),
  19 |   editors: [],
  20 |   activeEditor: undefined,
  21 |   messages: [],
  22 |   output: [],
  23 |   contextValues: new Map(),
  24 |   executed: [],
  25 |   quickPicks: [],
  26 |   lastQuickPick: undefined,
  27 |   quickPickAnswer: undefined,
  28 |   inputAnswers: [],
  29 |   informationAnswer: undefined,
  30 |   closeHandlers: [],
  31 |   changeHandlers: [],
  32 |   activeEditorHandlers: [],
  33 |   workspaceFolder: undefined,
  34 |   configuration: {},
  35 |   clipboard: [],
  36 |   semanticProviders: [],
  37 | };
  38 |
  39 | /** A VSCode-like URI with `file`, `magit` and provider documents. */
  40 | class Uri {
  41 |   /**
  42 |    * @param {{scheme: string, authority?: string, path?: string}} parts
  43 |    */
  44 |   constructor({ scheme, authority = '', path = '' }) {
  45 |     Object.assign(this, { scheme, authority, path });
  46 |   }
  47 |
  48 |   /** @param {object} parts */
  49 |   static from(parts) {
  50 |     return new Uri(parts);
  51 |   }
  52 |
  53 |   /** @param {string} value */
  54 |   static parse(value) {
  55 |     const match = /^([^:]+):\/\/([^/]*)(\/.*)$/.exec(value);
  56 |     if (!match) {
  57 |       return new Uri({ scheme: 'file', path: value });
  58 |     }
  59 |     return new Uri({ scheme: match[1], authority: match[2], path: match[3] });
  60 |   }
  61 |
  62 |   /** @param {string} file */
  63 |   static file(file) {
  64 |     return new Uri({ scheme: 'file', path: file });
  65 |   }
  66 |
  67 |   /** The local filesystem path of a `file` URI. */
  68 |   get fsPath() {
  69 |     return this.path;
  70 |   }
  71 |
  72 |   toString() {
  73 |     return `${this.scheme}://${this.authority}${this.path}`;
  74 |   }
  75 | }
  76 |
  77 | /** A VSCode-compatible event emitter. */
  78 | class EventEmitter {
  79 |   constructor() {
  80 |     this.listeners = [];
  81 |     this.event = (listener) => {
  82 |       this.listeners.push(listener);
  83 |       return { dispose: () => {} };
  84 |     };
  85 |   }
  86 |
  87 |   /** @param {unknown} value */
  88 |   fire(value) {
  89 |     for (const listener of this.listeners) listener(value);
  90 |   }
  91 |
  92 |   dispose() {
  93 |     this.listeners = [];
  94 |   }
  95 | }
  96 |
  97 | /** A zero-based text position. */
  98 | class Position {
  99 |   /** @param {number} line @param {number} character */
 100 |   constructor(line, character) {
 101 |     this.line = line;
 102 |     this.character = character;
 103 |   }
 104 | }
 105 |
 106 | /** A selection with an anchor and an active end. */
 107 | class Selection {
 108 |   /** @param {Position} anchor @param {Position} active */
 109 |   constructor(anchor, active) {
 110 |     this.anchor = anchor;
 111 |     this.active = active;
 112 |     const ordered = (left, right) => left.line < right.line
 113 |       || (left.line === right.line && left.character <= right.character);
 114 |     this.start = ordered(anchor, active) ? anchor : active;
 115 |     this.end = ordered(anchor, active) ? active : anchor;
 116 |     this.isEmpty = anchor.line === active.line && anchor.character === active.character;
 117 |   }
 118 | }
 119 |
 120 | /** A range between two positions. */
 121 | class Range {
 122 |   /** @param {Position} start @param {Position} end */
 123 |   constructor(start, end) {
 124 |     this.start = start;
 125 |     this.end = end;
 126 |   }
 127 | }
 128 |
 129 | /** Whole-document edits used by managed-draft history. */
 130 | class WorkspaceEdit {
 131 |   constructor() { this.edits = []; }
 132 |   /** Record a native replacement. */
 133 |   replace(uri, range, text) { this.edits.push({ uri, range, text }); }
 134 | }
 135 |
 136 | /** The semantic-token legend the server declares. */
 137 | class SemanticTokensLegend {
 138 |   /** @param {string[]} tokenTypes @param {string[]} tokenModifiers */
 139 |   constructor(tokenTypes, tokenModifiers) {
 140 |     this.tokenTypes = tokenTypes;
 141 |     this.tokenModifiers = tokenModifiers;
 142 |   }
 143 | }
 144 |
 145 | /** Decoded semantic tokens. */
 146 | class SemanticTokens {
 147 |   /** @param {Uint32Array} data @param {string} [resultId] */
 148 |   constructor(data, resultId) {
 149 |     this.data = data;
 150 |     this.resultId = resultId;
 151 |   }
 152 | }
 153 |
 154 | /**
 155 |  * Read the text a provider document currently exposes.
 156 |  *
 157 |  * @param {Uri} uri
 158 |  * @returns {string}
 159 |  */
 160 | function providerText(uri) {
 161 |   const provider = state.providers.get(uri.scheme);
 162 |   return provider ? provider.provideTextDocumentContent(uri) : '';
 163 | }
 164 |
 165 | /**
 166 |  * A text document over a provider URI, a real file or in-memory content.
 167 |  *
 168 |  * @param {Uri} uri
 169 |  * @param {string} content
 170 |  * @returns {object}
 171 |  */
 172 | function makeDocument(uri, content) {
 173 |   const lines = () => content.split('\n');
 174 |   const document = {
 175 |     uri,
 176 |     version: 1,
 177 |     isDirty: false,
 178 |     isClosed: false,
 179 |     languageId: 'plaintext',
 180 |     getText: () => content,
 181 |     get lineCount() {
 182 |       return lines().length;
 183 |     },
 184 |     lineAt(line) {
 185 |       return { text: lines()[line] || '' };
 186 |     },
 187 |     async save() {
 188 |       const provider = state.fileProviders.get(uri.scheme);
 189 |       if (provider) await provider.writeFile(uri, Buffer.from(content), {});
 190 |       this.isDirty = false;
 191 |       return true;
 192 |     },
 193 |     replaceText(text) {
 194 |       if (state.providers.has(uri.scheme)) return false;
 195 |       content = text;
 196 |       this.version += 1;
 197 |       this.isDirty = true;
 198 |       for (const handler of state.changeHandlers) handler({ document, contentChanges: [{ text }] });
 199 |       return true;
 200 |     },
 201 |   };
 202 |   return document;
 203 | }
 204 |
 205 | /**
 206 |  * Open or fetch the fake document for a URI.
 207 |  *
 208 |  * Provider documents are re-read from their provider on every access so a
 209 |  * `view/render` is visible exactly as a real virtual document reflects it.
 210 |  *
 211 |  * @param {Uri} uri
 212 |  * @param {string} [content]
 213 |  * @returns {object}
 214 |  */
 215 | function openDocument(uri, content) {
 216 |   const key = uri.toString();
 217 |   if (state.fileProviders.has(uri.scheme)) {
 218 |     const existing = state.documents.get(key);
 219 |     if (existing && !existing.isClosed) return existing;
 220 |     const document = makeDocument(uri, Buffer.from(state.fileProviders.get(uri.scheme).readFile(uri)).toString());
 221 |     state.documents.set(key, document);
 222 |     return document;
 223 |   }
 224 |   if (state.providers.has(uri.scheme)) {
 225 |     const text = providerText(uri);
 226 |     const document = makeDocument(uri, text);
 227 |     Object.defineProperty(document, 'getText', { value: () => providerText(uri) });
 228 |     Object.defineProperty(document, 'lineCount', {
 229 |       get: () => providerText(uri).split('\n').length,
 230 |     });
 231 |     Object.defineProperty(document, 'lineAt', {
 232 |       value: (line) => ({ text: providerText(uri).split('\n')[line] || '' }),
 233 |     });
 234 |     state.documents.set(key, document);
 235 |     return document;
 236 |   }
 237 |   const body = content !== undefined ? content : (fs.existsSync(uri.path) ? fs.readFileSync(uri.path, 'utf8') : '');
 238 |   const document = makeDocument(uri, body);
 239 |   state.documents.set(key, document);
 240 |   return document;
 241 | }
 242 |
 243 | /**
 244 |  * The editor currently showing a document, creating one when needed.
 245 |  *
 246 |  * @param {object} document
 247 |  * @returns {object}
 248 |  */
 249 | function editorFor(document) {
 250 |   let editor = state.editors.find(
 251 |     (candidate) => candidate.document.uri.toString() === document.uri.toString()
 252 |   );
 253 |   if (!editor) {
 254 |     const position = new Position(0, 0);
 255 |     editor = {
 256 |       document,
 257 |       selection: new Selection(position, position),
 258 |       revealRange() {},
 259 |       async edit(callback) {
 260 |         let text;
 261 |         callback({ replace(range, replacement) { text = replacement; } });
 262 |         return document.replaceText(text);
 263 |       },
 264 |     };
 265 |     state.editors.push(editor);
 266 |   }
 267 |   return editor;
 268 | }
 269 |
 270 | /**
 271 |  * Create a scriptable QuickPick.
 272 |  *
 273 |  * @returns {object}
 274 |  */
 275 | function createQuickPick() {
 276 |   const quickPick = {
 277 |     title: '',
 278 |     placeholder: '',
 279 |     items: [],
 280 |     selectedItems: [],
 281 |     value: '',
 282 |     visible: false,
 283 |     acceptHandler: undefined,
 284 |     changeHandler: undefined,
 285 |     hideHandler: undefined,
 286 |     show() {
 287 |       this.visible = true;
 288 |     },
 289 |     hide() {
 290 |       this.visible = false;
 291 |       if (this.hideHandler) this.hideHandler();
 292 |     },
 293 |     dispose() {
 294 |       this.visible = false;
 295 |     },
 296 |     onDidAccept(handler) {
 297 |       this.acceptHandler = handler;
 298 |     },
 299 |     onDidChangeValue(handler) {
 300 |       this.changeHandler = handler;
 301 |     },
 302 |     onDidHide(handler) {
 303 |       this.hideHandler = handler;
 304 |     },
 305 |   };
 306 |   Object.defineProperty(quickPick, 'value', {
 307 |     get() {
 308 |       return this._value || '';
 309 |     },
 310 |     set(next) {
 311 |       this._value = next;
 312 |       if (this.changeHandler) this.changeHandler(next);
 313 |     },
 314 |   });
 315 |   /** Select one item as a user would and fire accept. @param {object} item */
 316 |   quickPick.select = function select(item) {
 317 |     this.selectedItems = [item];
 318 |     if (this.acceptHandler) this.acceptHandler();
 319 |   };
 320 |   state.quickPicks.push(quickPick);
 321 |   state.lastQuickPick = quickPick;
 322 |   return quickPick;
 323 | }
 324 |
 325 | const vscode = {
 326 |   Uri,
 327 |   EventEmitter,
 328 |   Position,
 329 |   Selection,
 330 |   Range,
 331 |   WorkspaceEdit,
 332 |   SemanticTokensLegend,
 333 |   SemanticTokens,
 334 |   QuickPickItemKind: { Separator: -1, Default: 0 },
 335 |   TextEditorRevealType: { InCenterIfOutsideViewport: 0 },
 336 |   FileType: { File: 1 },
 337 |   FileChangeType: { Changed: 1 },
 338 |   FileSystemError: {
 339 |     FileNotFound: () => new Error('Draft not found'),
 340 |     NoPermissions: (text) => new Error(text),
 341 |   },
 342 |
 343 |   workspace: {
 344 |     get workspaceFolders() {
 345 |       return state.workspaceFolder
 346 |         ? [{ uri: Uri.file(state.workspaceFolder) }]
 347 |         : undefined;
 348 |     },
 349 |     getConfiguration(section) {
 350 |       return {
 351 |         /**
 352 |          * @param {string} name
 353 |          * @param {unknown} [fallback]
 354 |          */
 355 |         get(name, fallback) {
 356 |           const scoped = state.configuration[`${section}.${name}`];
 357 |           return scoped === undefined ? fallback : scoped;
 358 |         },
 359 |       };
 360 |     },
 361 |     registerTextDocumentContentProvider(scheme, provider) {
 362 |       state.providers.set(scheme, provider);
 363 |       return { dispose() {} };
 364 |     },
 365 |     registerFileSystemProvider(scheme, provider) {
 366 |       state.fileProviders.set(scheme, provider);
 367 |       return { dispose() {} };
 368 |     },
 369 |     onDidChangeTextDocument(handler) {
 370 |       state.changeHandlers.push(handler);
 371 |       return { dispose() {} };
 372 |     },
 373 |     async applyEdit(edit) {
 374 |       for (const replacement of edit.edits) {
 375 |         const document = state.documents.get(replacement.uri.toString());
 376 |         if (!document || document.isClosed || !document.replaceText(replacement.text)) return false;
 377 |       }
 378 |       return true;
 379 |     },
 380 |     onDidCloseTextDocument(handler) {
 381 |       state.closeHandlers.push(handler);
 382 |       return { dispose() {} };
 383 |     },
 384 |     async openTextDocument(input) {
 385 |       if (input instanceof Uri) {
 386 |         return openDocument(input);
 387 |       }
 388 |       return openDocument(Uri.from({ scheme: 'untitled', path: '/untitled' }), input.content || '');
 389 |     },
 390 |   },
 391 |
 392 |   languages: {
 393 |     async setTextDocumentLanguage(document, languageId) {
 394 |       document.languageId = languageId;
 395 |       // Real VSCode closes and reopens the document for a language change.
 396 |       for (const handler of state.closeHandlers) await handler(document);
 397 |       return document;
 398 |     },
 399 |     registerDocumentSemanticTokensProvider(selector, provider, legend) {
 400 |       state.semanticProviders.push({ selector, provider, legend });
 401 |       return { dispose() {} };
 402 |     },
 403 |   },
 404 |
 405 |   commands: {
 406 |     registerCommand(name, callback) {
 407 |       state.commands.set(name, callback);
 408 |       return { dispose() {} };
 409 |     },
 410 |     async executeCommand(name, ...args) {
 411 |       state.executed.push({ name, args });
 412 |       if (name === 'setContext') {
 413 |         state.contextValues.set(args[0], args[1]);
 414 |       }
 415 |       if (name === 'workbench.action.files.revert') {
 416 |         const uri = args[0] || state.activeEditor?.document.uri;
 417 |         const document = state.documents.get(uri.toString());
 418 |         const provider = state.fileProviders.get(uri.scheme);
 419 |         if (document && provider) {
 420 |           document.replaceText(Buffer.from(provider.readFile(uri)).toString());
 421 |           document.isDirty = false;
 422 |         }
 423 |       }
 424 |       if (state.commands.has(name)) return state.commands.get(name)(...args);
 425 |     },
 426 |   },
 427 |
 428 |   window: {
 429 |     get activeTextEditor() {
 430 |       return state.activeEditor;
 431 |     },
 432 |     get visibleTextEditors() {
 433 |       return state.editors;
 434 |     },
 435 |     tabGroups: {
 436 |       get all() {
 437 |         return state.editors.map((editor) => ({
 438 |           tabs: [{ input: { uri: editor.document.uri } }],
 439 |         }));
 440 |       },
 441 |       /** Managed drafts watch tab changes to catch raw tab close teardown. */
 442 |       onDidChangeTabs: new EventEmitter().event,
 443 |       async close(tabs) {
 444 |         for (const tab of Array.isArray(tabs) ? tabs : [tabs]) {
 445 |           const document = state.documents.get(tab.input.uri.toString());
 446 |           if (document) await vscode.__test.closeDocument(document);
 447 |         }
 448 |         return true;
 449 |       },
 450 |     },
 451 |     createOutputChannel() {
 452 |       return {
 453 |         /** @param {string} line */
 454 |         appendLine(line) {
 455 |           state.output.push(line);
 456 |         },
 457 |         dispose() {},
 458 |       };
 459 |     },
 460 |     onDidChangeActiveTextEditor(handler) {
 461 |       state.activeEditorHandlers.push(handler);
 462 |       return { dispose() {} };
 463 |     },
 464 |     async showTextDocument(document) {
 465 |       const editor = editorFor(document);
 466 |       state.activeEditor = editor;
 467 |       for (const handler of state.activeEditorHandlers) handler(editor);
 468 |       return editor;
 469 |     },
 470 |     createQuickPick,
 471 |     async showQuickPick(items) {
 472 |       if (state.quickPickAnswer) {
 473 |         return state.quickPickAnswer(items);
 474 |       }
 475 |       return items && items[0];
 476 |     },
 477 |     async showInputBox() {
 478 |       return state.inputAnswers.shift();
 479 |     },
 480 |     showErrorMessage(text) {
 481 |       state.messages.push({ level: 'error', text });
 482 |     },
 483 |     showWarningMessage(text) {
 484 |       state.messages.push({ level: 'warn', text });
 485 |     },
 486 |     showInformationMessage(text) {
 487 |       state.messages.push({ level: 'info', text });
 488 |       if (state.informationAnswer) return state.informationAnswer;
 489 |       return undefined;
 490 |     },
 491 |   },
 492 |
 493 |   env: {
 494 |     clipboard: {
 495 |       /** @param {string} text */
 496 |       async writeText(text) {
 497 |         state.clipboard.push(text);
 498 |       },
 499 |     },
 500 |   },
 501 |
 502 |   /** Test-only controls; the extension never calls these. */
 503 |   __test: {
 504 |     /** Drop every recorded interaction. */
 505 |     reset() {
 506 |       state.commands.clear();
 507 |       state.providers.clear();
 508 |       state.fileProviders.clear();
 509 |       state.documents.clear();
 510 |       state.editors.length = 0;
 511 |       state.activeEditor = undefined;
 512 |       state.messages.length = 0;
 513 |       state.output.length = 0;
 514 |       state.contextValues.clear();
 515 |       state.executed.length = 0;
 516 |       state.quickPicks.length = 0;
 517 |       state.lastQuickPick = undefined;
 518 |       state.quickPickAnswer = undefined;
 519 |       state.inputAnswers.length = 0;
 520 |       state.informationAnswer = undefined;
 521 |       state.closeHandlers.length = 0;
 522 |       state.changeHandlers.length = 0;
 523 |       state.activeEditorHandlers.length = 0;
 524 |       state.workspaceFolder = undefined;
 525 |       state.configuration = {};
 526 |       state.clipboard.length = 0;
 527 |       state.semanticProviders.length = 0;
 528 |     },
 529 |     get commands() {
 530 |       return state.commands;
 531 |     },
 532 |     get documents() {
 533 |       return state.documents;
 534 |     },
 535 |     get editors() {
 536 |       return state.editors;
 537 |     },
 538 |     get messages() {
 539 |       return state.messages;
 540 |     },
 541 |     get output() {
 542 |       return state.output;
 543 |     },
 544 |     get contextValues() {
 545 |       return state.contextValues;
 546 |     },
 547 |     get quickPicks() {
 548 |       return state.quickPicks;
 549 |     },
 550 |     get lastQuickPick() {
 551 |       return state.lastQuickPick;
 552 |     },
 553 |     get clipboard() {
 554 |       return state.clipboard;
 555 |     },
 556 |     get activeEditor() {
 557 |       return state.activeEditor;
 558 |     },
 559 |     /** @param {string} folder */
 560 |     setWorkspaceFolder(folder) {
 561 |       state.workspaceFolder = folder;
 562 |     },
 563 |     /** @param {Record<string, unknown>} values dotted `section.key` settings */
 564 |     setConfiguration(values) {
 565 |       Object.assign(state.configuration, values);
 566 |     },
 567 |     /** @param {string|undefined} answer */
 568 |     queueInputAnswer(answer) {
 569 |       state.inputAnswers.push(answer);
 570 |     },
 571 |     /** @param {(items: object[]) => unknown} answer */
 572 |     setQuickPickAnswer(answer) {
 573 |       state.quickPickAnswer = answer;
 574 |     },
 575 |     /** @param {unknown} answer */
 576 |     setInformationAnswer(answer) {
 577 |       state.informationAnswer = answer;
 578 |     },
 579 |     /** Simulate the user closing a document. @param {object} document */
 580 |     async closeDocument(document) {
 581 |       document.isClosed = true;
 582 |       state.editors = state.editors.filter((editor) => editor.document !== document);
 583 |       for (const handler of state.closeHandlers) await handler(document);
 584 |     },
 585 |   },
 586 | };
 587 |
 588 | module.exports = vscode;
```
### Unit/integration names and important actual seams


clients/vscode/test/test.js declarations/checks (all matches, source line):
`clients/vscode/test/test.js:24: function test(name, body) {`
`clients/vscode/test/test.js:35: test('a status descriptor derives the status scope', () => {`
`clients/vscode/test/test.js:48: test('a menu descriptor uses the shared menu scope and keeps capture', () => {`
`clients/vscode/test/test.js:58: test('text mode turns capture off', () => {`
`clients/vscode/test/test.js:68: test('no descriptor means inactive', () => {`
`clients/vscode/test/test.js:81: test(`render contract: ${vector.name}`, () => {`
`clients/vscode/test/test.js:140: test('QuickPick rows follow the menu groups', () => {`
`clients/vscode/test/test.js:168: test('typed characters become canonical key steps', () => {`
`clients/vscode/test/test.js:178: test('QuickPick rows preserve server activation steps for every canonical shape', () => {`
`clients/vscode/test/test.js:199: test('generated and mirrored bindings require editor focus and exclude widgets', () => {`
`clients/vscode/test/test.js:222: test('canonical status Enter, fold and refresh bindings keep the focus gate', () => {`
`clients/vscode/test/test.js:244: test('semantic-token types map to theme scopes', () => {`
`clients/vscode/test/test.js:290: test('one generic dispatcher carries every contributed action binding', () => {`
`clients/vscode/test/test.js:317: test('managed draft effects accept only the exact full-sync announcement', () => {`
`clients/vscode/test/test.js:346: test('JSONC comments and trailing commas are stripped', () => {`
`clients/vscode/test/test.js:352: test('JSONC parser preserves comment-like escaped strings and rejects malformed input', () => {`
`clients/vscode/test/test.js:358: test('native fold shortcuts map to magit.invoke entries', () => {`
`clients/vscode/test/test.js:381: test('fold levels become detail changes', () => {`
`clients/vscode/test/test.js:391: test('shared prompt vectors preserve protocol answer semantics', () => {`
`clients/vscode/test/test.js:420: test('executable VSCode e2e paths enforce no-vision policy', noVisionPolicy);`

clients/vscode/test/integration.js declarations/checks (all matches, source line):
`clients/vscode/test/integration.js:131: function delay(milliseconds) {`
`clients/vscode/test/integration.js:143: async function waitFor(condition, label, timeout = 10000) {`
`clients/vscode/test/integration.js:159: function git(repo, args) {`
`clients/vscode/test/integration.js:168: function createRepository() {`
`clients/vscode/test/integration.js:185: function loadExtension() {`
`clients/vscode/test/integration.js:187: Module._load = function load(request, parent, isMain) {`
`clients/vscode/test/integration.js:200: async function run() {`

clients/vscode/test/open.test.js declarations/checks (all matches, source line):
`clients/vscode/test/open.test.js:7: module.exports = async function testStatusOpenLifecycle() {`
`clients/vscode/test/open.test.js:155: Module._load = function load(request, parent, isMain) {`

clients/vscode/test/selection-integration.js declarations/checks (all matches, source line):
`clients/vscode/test/selection-integration.js:12: function select(vscode, editor, anchor, active) {`
`clients/vscode/test/selection-integration.js:23: function checkRequest(records, before, method, active, rows) {`
`clients/vscode/test/selection-integration.js:40: async function checkShapes({ vscode, commands, client, editor }) {`
`clients/vscode/test/selection-integration.js:48: client.sendRequest = function sendRequest(method, params) {`
`clients/vscode/test/selection-integration.js:103: function git(repo, ...args) {`
`clients/vscode/test/selection-integration.js:108: async function checkGitSelections({ vscode, commands, client }) {`
`clients/vscode/test/selection-integration.js:249: module.exports = { checkShapes, checkGitSelections };`

`test.js` test registrations and core logic: full file is 436 lines; function/name index above is exhaustive for declarations/tests. Parameterized test implementation and cases are here:

### `clients/vscode/test/test.js` lines 1-100 of 436
```text
   1 | 'use strict';
   2 |
   3 | const assert = require('node:assert/strict');
   4 |
   5 | const context = require('../lib/context');
   6 | const render = require('../lib/render');
   7 | const menu = require('../lib/menu');
   8 | const foldkeys = require('../lib/foldkeys');
   9 | const prompt = require('../lib/prompt');
  10 | const edit = require('../lib/edit');
  11 | const promptVectors = require('../../../tests/data/client-contracts.json').prompts;
  12 | const noVisionPolicy = require('./no-vision');
  13 | const renderVectors = require('../../../tests/data/client-contracts.json').renders;
  14 |
  15 | const failures = [];
  16 | let passed = 0;
  17 |
  18 | /**
  19 |  * Run one named test and record its failure instead of stopping the suite.
  20 |  *
  21 |  * @param {string} name
  22 |  * @param {() => void} body
  23 |  */
  24 | function test(name, body) {
  25 |   try {
  26 |     body();
  27 |     passed += 1;
  28 |   } catch (error) {
  29 |     failures.push({ name, error });
  30 |   }
  31 | }
  32 |
  33 | // lib/context.js -----------------------------------------------------------
  34 |
  35 | test('a status descriptor derives the status scope', () => {
  36 |   const keys = context.deriveContext(
  37 |     { id: 'v1', scope: 'status', capture: false },
  38 |     'interactive'
  39 |   );
  40 |   assert.deepEqual(keys, {
  41 |     'magit.active': true,
  42 |     'magit.scope': 'status',
  43 |     'magit.textMode': false,
  44 |     'magit.capture': false,
  45 |   });
  46 | });
  47 |
  48 | test('a menu descriptor uses the shared menu scope and keeps capture', () => {
  49 |   const keys = context.deriveContext(
  50 |     { id: 'v2', scope: 'menu.commit', capture: true },
  51 |     'interactive'
  52 |   );
  53 |   assert.equal(keys['magit.scope'], 'menu');
  54 |   assert.equal(keys['magit.capture'], true);
  55 |   assert.equal(keys['magit.textMode'], false);
  56 | });
  57 |
  58 | test('text mode turns capture off', () => {
  59 |   const keys = context.deriveContext(
  60 |     { id: 'v3', scope: 'menu.rebase', capture: true },
  61 |     'text'
  62 |   );
  63 |   assert.equal(keys['magit.scope'], 'menu');
  64 |   assert.equal(keys['magit.textMode'], true);
  65 |   assert.equal(keys['magit.capture'], false);
  66 | });
  67 |
  68 | test('no descriptor means inactive', () => {
  69 |   assert.deepEqual(context.deriveContext(undefined, 'interactive'), {
  70 |     'magit.active': false,
  71 |     'magit.scope': '',
  72 |     'magit.textMode': false,
  73 |     'magit.capture': false,
  74 |   });
  75 |   assert.equal(context.inactiveContext()['magit.active'], false);
  76 | });
  77 |
  78 | // lib/render.js ------------------------------------------------------------
  79 |
  80 | for (const vector of renderVectors) {
  81 |   test(`render contract: ${vector.name}`, () => {
  82 |     const result = render.applyRender(vector.initialLines, vector.initialVersion, vector.render);
  83 |     assert.deepEqual(result.lines, vector.expectedLines);
  84 |     assert.equal(result.version, vector.expectedVersion);
  85 |     assert.equal(result.status, vector.outcome === 'resync' ? 'resync' : vector.render.text ? 'full' : vector.render.edits ? 'edits' : 'none');
  86 |   });
  87 | }
  88 |
  89 | // lib/menu.js --------------------------------------------------------------
  90 |
  91 | const MENU_MODEL = {
  92 |   id: 'commit',
  93 |   title: 'Commit',
  94 |   groups: [
  95 |     {
  96 |       title: 'Arguments',
  97 |       items: [
  98 |         {
  99 |           kind: 'switch',
 100 |           key: '-a',
```
### `clients/vscode/test/test.js` lines 135-245 of 436
```text
 135 |     },
 136 |   ],
 137 |   savedArgs: { switches: [], options: {} },
 138 | };
 139 |
 140 | test('QuickPick rows follow the menu groups', () => {
 141 |   const rows = menu.quickPickItems(MENU_MODEL);
 142 |   assert.equal(rows.length, 6);
 143 |
 144 |   assert.equal(rows[0].kind, 'separator');
 145 |   assert.equal(rows[0].label, 'Arguments');
 146 |
 147 |   assert.equal(rows[1].kind, 'item');
 148 |   assert.equal(rows[1].key, '-a');
 149 |   assert.equal(rows[1].label, '-a  Stage all modified and deleted files');
 150 |   assert.match(rows[1].description, /--all/);
 151 |   assert.match(rows[1].description, /on/);
 152 |   assert.equal(rows[1].on, true);
 153 |
 154 |   assert.equal(rows[2].key, '=A');
 155 |   assert.equal(rows[2].value, 'Alice');
 156 |   assert.match(rows[2].description, /--author=/);
 157 |
 158 |   assert.equal(rows[3].label, 'Create');
 159 |   assert.equal(rows[4].key, 'c');
 160 |   assert.equal(rows[4].status, 'implemented');
 161 |
 162 |   assert.equal(rows[5].key, 'e');
 163 |   assert.equal(rows[5].status, 'planned');
 164 |   assert.equal(rows[5].roadmapId, 'R-4.1');
 165 |   assert.match(rows[5].description, /planned R-4\.1/);
 166 | });
 167 |
 168 | test('typed characters become canonical key steps', () => {
 169 |   assert.deepEqual(menu.routeTyped('c'), ['c']);
 170 |   assert.deepEqual(menu.routeTyped('-a'), ['-', 'a']);
 171 |   assert.deepEqual(menu.routeTyped('=A'), ['=', 'A']);
 172 |   assert.deepEqual(menu.routeTyped('?'), ['?']);
 173 |   assert.deepEqual(menu.routeTyped(' '), ['Space']);
 174 |   assert.deepEqual(menu.routeTyped('+'), ['Plus']);
 175 |   assert.deepEqual(menu.routeTyped(undefined), []);
 176 | });
 177 |
 178 | test('QuickPick rows preserve server activation steps for every canonical shape', () => {
 179 |   const cases = [
 180 |     ['-a', ['-', 'a']], ['=A', ['=', 'A']], ['g l', ['g', 'l']],
 181 |     ['ctrl+t', ['ctrl+t']], ['Enter', ['Enter']], ['G', ['G']], ['Plus', ['Plus']],
 182 |   ];
 183 |   for (const [key, activationSteps] of cases) {
 184 |     const item = { kind: 'action', key, activationSteps, label: 'Planned',
 185 |       status: { status: 'planned', roadmapId: 'R-4.1' } };
 186 |     const row = menu.quickPickItems({ groups: [{ title: 'Actions', items: [item] }] })[1];
 187 |     assert.equal(row.key, key);
 188 |     assert.equal(row.activationSteps, activationSteps, 'no client parsing or conversion');
 189 |     assert.equal(row.status, 'planned');
 190 |     assert.equal(row.roadmapId, 'R-4.1');
 191 |   }
 192 |   assert.deepEqual(menu.routeTyped('ctrl+t'), ['c', 't', 'r', 'l', 'Plus', 't']);
 193 |   assert.deepEqual(menu.routeTyped('g l'), ['g', 'Space', 'l']);
 194 |   assert.deepEqual(menu.routeTyped('Enter'), ['E', 'n', 't', 'e', 'r']);
 195 | });
 196 |
 197 | // lib/foldkeys.js ----------------------------------------------------------
 198 |
 199 | test('generated and mirrored bindings require editor focus and exclude widgets', () => {
 200 |   const generated = require('../package.json').contributes.keybindings;
 201 |   const mirrored = foldkeys.mapFoldKeys([{ key: 'ctrl+k ctrl+l', command: 'editor.toggleFold' }]);
 202 |   for (const entry of [...generated, ...mirrored]) {
 203 |     const terms = entry.when.split(' && ');
 204 |     for (const term of ['magit.active', 'editorTextFocus', '!quickInputVisible',
 205 |       '!suggestWidgetVisible', '!findWidgetVisible', '!renameInputVisible']) {
 206 |       assert(terms.includes(term), `${entry.key}: missing ${term}`);
 207 |     }
 208 |     // Test the focus gate independently of scope/text-mode variants.
 209 |     const gate = (state) => terms.every((term) => {
 210 |       if (term.startsWith('magit.scope') || term.includes('magit.textMode')) return true;
 211 |       return term.startsWith('!') ? !state[term.slice(1)] : !!state[term];
 212 |     });
 213 |     const status = { 'magit.active': true, editorTextFocus: true };
 214 |     assert(gate(status));
 215 |     assert(!gate({ ...status, editorTextFocus: false }), 'sidebar/terminal/input focus');
 216 |     for (const widget of ['quickInputVisible', 'suggestWidgetVisible', 'findWidgetVisible', 'renameInputVisible']) {
 217 |       assert(!gate({ ...status, [widget]: true }), widget);
 218 |     }
 219 |   }
 220 | });
 221 |
 222 | test('canonical status Enter, fold and refresh bindings keep the focus gate', () => {
 223 |   const generated = require('../package.json').contributes.keybindings;
 224 |   const required = [
 225 |     ['enter', 'common.visit'],
 226 |     ['tab', 'common.toggle-section'],
 227 |     ['g', 'common.refresh'],
 228 |     ['f5', 'common.refresh'],
 229 |   ];
 230 |   for (const [key, command] of required) {
 231 |     const matches = generated.filter((entry) => entry.key === key
 232 |       && entry.args && entry.args.command === command
 233 |       && entry.when.includes("magit.scope == 'status'"));
 234 |     assert.equal(matches.length, 1, `${key} -> ${command} for status`);
 235 |     const terms = matches[0].when.split(' && ');
 236 |     for (const term of ['magit.active', 'editorTextFocus', '!quickInputVisible',
 237 |       '!suggestWidgetVisible', '!findWidgetVisible', '!renameInputVisible',
 238 |       '!magit.textMode']) {
 239 |       assert(terms.includes(term), `${key}: missing ${term}`);
 240 |     }
 241 |   }
 242 | });
 243 |
 244 | test('semantic-token types map to theme scopes', () => {
 245 |   const contributions = require('../package.json').contributes;
```
### `clients/vscode/test/test.js` lines 285-435 of 436
```text
 285 |     true,
 286 |     'semantic highlighting is not enabled for the magit language'
 287 |   );
 288 | });
 289 |
 290 | test('one generic dispatcher carries every contributed action binding', () => {
 291 |   const contributions = require('../package.json').contributes;
 292 |   const clientCommands = contributions.commands.map((entry) => entry.command).sort();
 293 |   assert.deepEqual(clientCommands, [
 294 |     'magit.cancelEdit',
 295 |     'magit.closeEdit',
 296 |     'magit.confirmEdit',
 297 |     'magit.invoke',
 298 |     'magit.key',
 299 |     'magit.mirrorFoldShortcuts',
 300 |     'magit.open',
 301 |   ]);
 302 |   // No contributed VSCode command may be named after a server action; every
 303 |   // action binding goes through `magit.invoke` with a canonical server ID.
 304 |   for (const perAction of ['magit.refresh', 'magit.toggle-fold', 'magit.show-section',
 305 |     'magit.hide-section', 'magit.stage']) {
 306 |     assert(!clientCommands.includes(perAction), perAction);
 307 |   }
 308 |   assert.equal(contributions.keybindings.length > 100, true);
 309 |   for (const binding of contributions.keybindings) {
 310 |     assert.equal(binding.command, 'magit.invoke', binding.key);
 311 |     assert.equal(typeof binding.args?.command, 'string', binding.key);
 312 |     assert.match(binding.args.command, /^[a-z][a-z0-9-]*\.[a-z][a-z0-9-]*$/);
 313 |     assert(!clientCommands.includes(binding.args.command), binding.key);
 314 |   }
 315 | });
 316 |
 317 | test('managed draft effects accept only the exact full-sync announcement', () => {
 318 |   const effect = { type: 'editDocument', mode: 'realFile', runId: 'run-1',
 319 |     uri: 'file:///repo/.git/COMMIT_EDITMSG', version: 1, text: 'exact\r\n', kind: 'commit-message' };
 320 |   assert(edit.isEditEffect(effect));
 321 |   for (const invalid of [{ ...effect, path: '/tmp/message' }, { ...effect, mode: 'unsupported' },
 322 |     { ...effect, uri: 'magit-commit://other/message' }, { ...effect, version: 2 },
 323 |     { ...effect, requestId: 4 }]) assert(!edit.isEditEffect(invalid));
 324 |   assert.equal(edit.editingContext(effect)['magit.active'], false, 'native typing is not a view keymap');
 325 |   assert.equal(edit.draftAction('common.quit'), 'close');
 326 |   assert.equal(edit.draftAction('commit-message.history-prev'), undefined);
 327 |   assert.equal(edit.draftAction('commit-message.history-next'), undefined);
 328 | });
 329 |
 330 | const KEYBINDINGS_JSONC = `[
 331 |   // Fold the region under the cursor.
 332 |   { "key": "ctrl+shift+[", "command": "editor.fold" },
 333 |   /* Unfold it again. */
 334 |   { "key": "ctrl+shift+]", "command": "editor.unfold", "when": "editorTextFocus" },
 335 |   { "key": "ctrl+k ctrl+l", "command": "editor.toggleFold" },
 336 |   { "key": "ctrl+k ctrl+0", "command": "editor.foldAll" },
 337 |   { "key": "ctrl+shift+p", "command": "workbench.action.showCommands" },
 338 |   {
 339 |     "key": "ctrl+shift+[",
 340 |     "command": "editor.foldRecursively",
 341 |     "mac": "cmd+alt+[",
 342 |     "args": { "note": "https://example.com // not a comment" }
 343 |   },
 344 | ]`;
 345 |
 346 | test('JSONC comments and trailing commas are stripped', () => {
 347 |   const parsed = foldkeys.parseJsonc(KEYBINDINGS_JSONC);
 348 |   assert.equal(parsed.length, 6);
 349 |   assert.equal(parsed[5].command, 'editor.foldRecursively');
 350 | });
 351 |
 352 | test('JSONC parser preserves comment-like escaped strings and rejects malformed input', () => {
 353 |   const parsed = foldkeys.parseJsonc('[{"key":"x\\\"//text","command":"editor.fold","note":"/* text */",},]');
 354 |   assert.equal(parsed[0].key, 'x"//text');
 355 |   assert.throws(() => foldkeys.parseJsonc('[{"command":]'), /Invalid JSONC.*line/);
 356 | });
 357 |
 358 | test('native fold shortcuts map to magit.invoke entries', () => {
 359 |   const entries = foldkeys.mapFoldKeys(KEYBINDINGS_JSONC);
 360 |   assert.equal(entries.length, 5);
 361 |
 362 |   assert.deepEqual(entries[0], {
 363 |     key: 'ctrl+shift+[',
 364 |     command: 'magit.invoke',
 365 |     args: { command: 'common.hide-section' },
 366 |     when: 'magit.active && !magit.textMode && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible',
 367 |   });
 368 |   assert.equal(entries[1].args.command, 'common.show-section');
 369 |   assert.equal(entries[2].args.command, 'common.toggle-section');
 370 |   assert.equal(entries[3].args.command, 'common.hide-all');
 371 |
 372 |   // The workbench command is not a fold shortcut: order skips it.
 373 |   assert.equal(entries[4].key, 'ctrl+shift+[');
 374 |   assert.deepEqual(entries[4].args, {
 375 |     command: 'common.hide-section',
 376 |     args: { recursive: true },
 377 |   });
 378 |   assert.equal(entries[4].mac, 'cmd+alt+[');
 379 | });
 380 |
 381 | test('fold levels become detail changes', () => {
 382 |   const entries = foldkeys.mapFoldKeys([
 383 |     { key: 'ctrl+k ctrl+1', command: 'editor.foldLevel1' },
 384 |   ]);
 385 |   assert.equal(entries.length, 1);
 386 |   assert.equal(entries[0].args.command, 'common.less-detail');
 387 | });
 388 |
 389 | // lib/prompt.js ------------------------------------------------------------
 390 |
 391 | test('shared prompt vectors preserve protocol answer semantics', () => {
 392 |   for (const vector of promptVectors) {
 393 |     const params = vector.params;
 394 |     let answer;
 395 |     if (params.kind === 'confirm') {
 396 |       const items = prompt.promptItems(params);
 397 |       if (vector.focus) {
 398 |         assert.deepEqual(items.map((item) => item.label), vector.focus, vector.name);
 399 |       }
 400 |       answer = prompt.answerFor(params, vector.response === null
 401 |         ? undefined : { id: vector.response });
 402 |     } else if (params.kind === 'text') {
 403 |       answer = prompt.answerFor(params, vector.response);
 404 |     } else if (params.kind === 'choice') {
 405 |       answer = vector.response === '__custom__'
 406 |         ? prompt.customChoice(vector.customResponse)
 407 |         : prompt.answerFor(params, vector.response === null
 408 |           ? undefined : { id: vector.response });
 409 |     } else {
 410 |       answer = vector.response === null
 411 |         ? { cancelled: true }
 412 |         : prompt.answerFor(params, vector.response.map((id) => ({ id })));
 413 |     }
 414 |     assert.deepEqual(answer, vector.answer, vector.name);
 415 |   }
 416 | });
 417 |
 418 | // summary ------------------------------------------------------------------
 419 |
 420 | test('executable VSCode e2e paths enforce no-vision policy', noVisionPolicy);
 421 |
 422 | if (failures.length > 0) {
 423 |   for (const failure of failures) {
 424 |     console.error(`FAIL: ${failure.name}`);
 425 |     console.error(failure.error && failure.error.stack ? failure.error.stack : failure.error);
 426 |   }
 427 |   console.error(`\n${failures.length} failed, ${passed} passed`);
 428 |   process.exit(1);
 429 | }
 430 |
 431 | require('./open.test')().then(() => {
 432 |   console.log(`ok ${passed} tests`);
 433 | }).catch((error) => {
 434 |   console.error(error && error.stack ? error.stack : error);
 435 |   process.exitCode = 1;
```

`integration.js` actual named run and status-open/refresh/idle/document-text/env seams:

### `clients/vscode/test/integration.js` lines 1-40 of 653
```text
   1 | 'use strict';
   2 |
   3 | /**
   4 |  * Node integration test: activate the real extension against the real server.
   5 |  *
   6 |  * A scripted `vscode` module supplies the editor widgets and virtual
   7 |  * documents; the fake `LanguageClient` speaks LSP to `target/debug/magit-lsp`
   8 |  * over stdio. The test proves the thin-client contract end to end: open a view,
   9 |  * render the server's text verbatim, press a key that triggers a render, use a
  10 |  * menu QuickPick, and answer a server prompt.
  11 |  */
  12 |
  13 | const assert = require('node:assert/strict');
  14 | const { execFileSync } = require('node:child_process');
  15 | const fs = require('node:fs');
  16 | const os = require('node:os');
  17 | const path = require('node:path');
  18 | const Module = require('node:module');
  19 |
  20 | const vscode = require('./fake/vscode');
  21 | const { LanguageClient, instances } = require('./fake/languageclient');
  22 |
  23 | /** Scripted client with explicit, validated work-done progress support. */
  24 | class IntegrationClient extends LanguageClient {
  25 |   constructor(id, name, serverOptions, clientOptions) {
  26 |     const initialize = clientOptions.middleware.initialize;
  27 |     super(id, name, serverOptions, {
  28 |       ...clientOptions,
  29 |       middleware: {
  30 |         ...clientOptions.middleware,
  31 |         initialize(params, next, token) {
  32 |           params.capabilities.window = { workDoneProgress: true };
  33 |           return initialize(params, next, token);
  34 |         },
  35 |       },
  36 |     });
  37 |     this.createdProgress = new Set();
  38 |     this.progress = new Map();
  39 |     this.onNotification('$/progress', ({ token, value }) => {
  40 |       assert(this.createdProgress.has(token), 'progress must be created before use');
```
### `clients/vscode/test/integration.js` lines 131-215 of 653
```text
 131 | function delay(milliseconds) {
 132 |   return new Promise((resolve) => setTimeout(resolve, milliseconds));
 133 | }
 134 |
 135 | /**
 136 |  * Poll `condition` until it is truthy or the timeout expires.
 137 |  *
 138 |  * @param {() => unknown} condition
 139 |  * @param {string} label
 140 |  * @param {number} [timeout]
 141 |  * @returns {Promise<void>}
 142 |  */
 143 | async function waitFor(condition, label, timeout = 10000) {
 144 |   const deadline = Date.now() + timeout;
 145 |   while (!condition()) {
 146 |     if (Date.now() > deadline) {
 147 |       throw new Error(`timed out waiting for ${label}`);
 148 |     }
 149 |     await delay(20);
 150 |   }
 151 | }
 152 |
 153 | /**
 154 |  * Run Git in the fixture repository.
 155 |  *
 156 |  * @param {string} repo
 157 |  * @param {string[]} args
 158 |  */
 159 | function git(repo, args) {
 160 |   execFileSync('git', ['-C', repo, ...args], { stdio: 'ignore' });
 161 | }
 162 |
 163 | /**
 164 |  * Create a repository with one modified tracked file.
 165 |  *
 166 |  * @returns {string} repository path
 167 |  */
 168 | function createRepository() {
 169 |   const repo = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-lsp-vscode-'));
 170 |   git(repo, ['init', '-q']);
 171 |   git(repo, ['config', 'user.name', 'Test']);
 172 |   git(repo, ['config', 'user.email', 'test@example.com']);
 173 |   fs.writeFileSync(path.join(repo, 'tracked.txt'), 'original\n');
 174 |   git(repo, ['add', '.']);
 175 |   git(repo, ['commit', '-qm', 'initial']);
 176 |   fs.writeFileSync(path.join(repo, 'tracked.txt'), 'changed\n');
 177 |   return repo;
 178 | }
 179 |
 180 | /**
 181 |  * Load `extension.js` with the fake editor modules installed.
 182 |  *
 183 |  * @returns {object} the extension module
 184 |  */
 185 | function loadExtension() {
 186 |   const originalLoad = Module._load;
 187 |   Module._load = function load(request, parent, isMain) {
 188 |     if (request === 'vscode') return vscode;
 189 |     if (request === 'vscode-languageclient/node') return { LanguageClient: IntegrationClient };
 190 |     return originalLoad.call(this, request, parent, isMain);
 191 |   };
 192 |   try {
 193 |     return require('../extension');
 194 |   } finally {
 195 |     Module._load = originalLoad;
 196 |   }
 197 | }
 198 |
 199 | /** Run the scenario, reporting product output when an assertion fails. */
 200 | async function run() {
 201 |   const binary = process.env.MAGIT_LSP_SERVER_PATH
 202 |     || path.resolve(here, '../../../target/debug/magit-lsp');
 203 |   if (!fs.existsSync(binary)) {
 204 |     throw new Error(`missing ${binary}; run \`cargo build\` or \`make build\` first`);
 205 |   }
 206 |
 207 |   const repo = createRepository();
 208 |   vscode.__test.reset();
 209 |   vscode.workspace.registerFileSystemProvider('file', {
 210 |     readFile: (uri) => fs.readFileSync(uri.fsPath),
 211 |     writeFile: (uri, bytes) => fs.writeFileSync(uri.fsPath, bytes),
 212 |   });
 213 |   vscode.__test.setWorkspaceFolder(repo);
 214 |   vscode.__test.setConfiguration({
 215 |     'magit-lsp.serverPath': binary,
```
### `clients/vscode/test/integration.js` lines 228-320 of 653
```text
 228 |     // 1. Open the status view and display the server's text verbatim.
 229 |     await commands.get('magit.open')();
 230 |     await waitFor(
 231 |       () => vscode.__test.activeEditor
 232 |         && /Unstaged changes/.test(vscode.__test.activeEditor.document.getText()),
 233 |       'status view text'
 234 |     );
 235 |     const statusEditor = vscode.__test.activeEditor;
 236 |     const openedText = statusEditor.document.getText();
 237 |     assert.match(openedText, /Unstaged changes/);
 238 |     assert.equal(statusEditor.document.uri.scheme, 'magit');
 239 |     assert(
 240 |       client.requests.some((request) => request.method === 'view/open'),
 241 |       'opened the view through view/open'
 242 |     );
 243 |     assert(
 244 |       client.notes.some((note) => note.method === 'view/rendered'),
 245 |       'acknowledged the first render'
 246 |     );
 247 |
 248 |     // 2. A key press goes to the server and its render replaces the text.
 249 |     fs.writeFileSync(path.join(repo, 'added.txt'), 'new\n');
 250 |     const rendersBefore = client.notes.filter((note) => note.method === 'view/rendered').length;
 251 |     await commands.get('magit.key')({ key: 'g' });
 252 |     await waitFor(
 253 |       () => /added\.txt/.test(statusEditor.document.getText()),
 254 |       'refreshed status text'
 255 |     );
 256 |     assert(
 257 |       client.requests.some((request) => request.method === 'view/key'
 258 |         && request.params.key === 'g'
 259 |         && request.params.viewId === 'v1'),
 260 |       'forwarded the canonical key with the server view id'
 261 |     );
 262 |     assert(
 263 |       client.notes.filter((note) => note.method === 'view/rendered').length > rendersBefore,
 264 |       'acknowledged the render caused by the key press'
 265 |     );
 266 |
 267 |     await require('./selection-integration').checkShapes({ vscode, commands, client, editor: statusEditor });
 268 |
 269 |     // Static mode is a metadata-only transition: it must not invalidate the
 270 |     // virtual document or disturb the user's selection.
 271 |     const statusState = require('../lib/store').store.views.get('v1');
 272 |     const selection = new vscode.Selection(new vscode.Position(0, 0), new vscode.Position(0, 3));
 273 |     statusEditor.selection = selection;
 274 |     const linesBeforeStatic = statusState.lines;
 275 |     const versionBeforeStatic = statusState.version;
 276 |     let documentsChangedCount = 0;
 277 |     const documentsChangedSubscription = require('../lib/store').store.documentsChanged.event(() => {
 278 |       documentsChangedCount += 1;
 279 |     });
 280 |     const changedBeforeStatic = client.notes.filter((note) => note.method === 'view/rendered').length;
 281 |     await commands.get('magit.invoke')({ command: 'common.toggle-text-mode' });
 282 |     await waitFor(() => vscode.__test.contextValues.get('magit.textMode') === true,
 283 |       'text-mode context');
 284 |     assert.equal(vscode.__test.contextValues.get('magit.textMode'), true);
 285 |     assert.equal(statusEditor.selection, selection);
 286 |     assert.equal(statusState.lines, linesBeforeStatic);
 287 |     assert.equal(statusState.version, versionBeforeStatic);
 288 |     assert.equal(documentsChangedCount, 0);
 289 |     assert.equal(client.notes.filter((note) => note.method === 'view/rendered').length, changedBeforeStatic + 1);
 290 |     assert.equal(client.notes.at(-1).method, 'view/rendered');
 291 |     assert.equal(client.notes.at(-1).params.version, versionBeforeStatic);
 292 |
 293 |     // Refreshes queued while static are applied as a full server render on
 294 |     // return to interactive mode.
 295 |     fs.writeFileSync(path.join(repo, 'queued.txt'), 'queued\n');
 296 |     await commands.get('magit.key')({ key: 'g' });
 297 |     assert.equal(statusState.lines, linesBeforeStatic);
 298 |     assert.equal(statusEditor.selection, selection);
 299 |     await commands.get('magit.invoke')({ command: 'common.toggle-text-mode' });
 300 |     await waitFor(() => vscode.__test.contextValues.get('magit.textMode') === false,
 301 |       'interactive-mode context');
 302 |     await waitFor(() => /queued\.txt/.test(statusEditor.document.getText()), 'queued refresh after static mode');
 303 |     assert.equal(vscode.__test.contextValues.get('magit.textMode'), false);
 304 |     assert.notEqual(statusState.lines, linesBeforeStatic);
 305 |     documentsChangedSubscription.dispose();
 306 |
 307 |     // 3. A server menu becomes a QuickPick; selecting a row sends its key.
 308 |     let promptItems;
 309 |     vscode.__test.setQuickPickAnswer((items) => {
 310 |       promptItems = items;
 311 |       return items.find((item) => item.id === 'no');
 312 |     });
 313 |     await commands.get('magit.invoke')({ command: 'open.commit' });
 314 |     await waitFor(
 315 |       () => vscode.__test.lastQuickPick
 316 |         && vscode.__test.lastQuickPick.items.some((item) => item.key === 'c'),
 317 |       'commit menu QuickPick'
 318 |     );
 319 |     const menu = vscode.__test.lastQuickPick;
 320 |     const initialMenuState = require('../lib/store').store.views.get(
```
### `clients/vscode/test/integration.js` lines 425-445 of 653
```text
 425 |     const { store: draftStore } = require('../lib/store');
 426 |     const progress = client.progress;
 427 |     assert.equal(client.requests.find((request) => request.method === 'initialize')
 428 |       .params.capabilities.window.workDoneProgress, true);
 429 |     const draftProgress = new WeakMap();
 430 |     const head = () => execFileSync('git', ['-C', repo, 'rev-parse', 'HEAD']).toString();
 431 |     const subject = () => execFileSync('git', ['-C', repo, 'log', '-1', '--format=%s']).toString().trim();
 432 |     const idle = async (draft) => {
 433 |       const token = draftProgress.get(draft);
 434 |       assert.notEqual(token, undefined, 'draft must have observed progress begin');
 435 |       await draft.queue;
 436 |       await waitFor(() => progress.get(token) === 'end', 'commit progress end');
 437 |     };
 438 |     let probe = 0;
 439 |     let previousDraft;
 440 |     const startDraft = async () => {
 441 |       if (previousDraft) await idle(previousDraft);
 442 |       await vscode.window.showTextDocument(statusEditor.document);
 443 |       fs.writeFileSync(path.join(repo, 'tracked.txt'), `managed probe ${++probe}\n`);
 444 |       git(repo, ['add', 'tracked.txt']);
 445 |       const before = new Set(draftStore.drafts.keys());
```
