'use strict';

const vscode = require('vscode');
const fs = require('node:fs');
const path = require('node:path');
const { LanguageClient } = require('vscode-languageclient/node');

const {
  openStatus,
  pressCommand,
  invokeCommand,
  mirrorFoldShortcuts,
} = require('./commands');
const {
  confirmEditCommand,
  cancelEditCommand,
  onDocumentClosed,
  onActiveEditorChanged,
} = require('./edits');
const { close, current, disconnect, handleInvoke, onChanged, setup } = require('./drafts');
const { applyEffects } = require('./effects');
const { applyRender, handlePrompt, handlePick } = require('./requests');
const { register } = require('./semantic');
const { resolveServer } = require('./server-download');
const { optionalEffects, optionalPromptKinds } = require('../types/shared-lists');
const { activateView, stateForUri } = require('./views');
const { SCHEME, store } = require('./store');
const { message } = require('./util');
const { PROTOCOL_VERSION, protocolError, resolveCommand, warning } = require('./version');
const extensionVersion = JSON.parse(fs.readFileSync(path.join(__dirname, '../package.json'), 'utf8')).version;

/**
 * Create the LSP client with the static key-delivery capabilities.
 *
 * @param {string} command executable or PATH name the client starts
 * @param {string} profile keymap profile sent as an initialization option
 * @returns {LanguageClient}
 */
function createClient(command, profile) {
  const client = new LanguageClient(
    'magit-lsp',
    'magit-lsp',
    { command, args: [] },
    {
      documentSelector: [{ scheme: SCHEME }],
      initializationOptions: { profile },
      uriConverters: {
        code2Protocol: (uri) => stateForUri(uri)?.descriptor.uri || uri.toString(),
        protocol2Code: (uri) => Array.from(store.views.values())
          .find((state) => state.descriptor.uri === uri)?.uri || vscode.Uri.parse(uri),
      },
      middleware: {
        // The explicit provider maps dotted server types to legal VSCode IDs.
        provideDocumentSemanticTokens: () => null,
      },
    }
  );
  client.registerFeature({
    fillClientCapabilities(capabilities) {
      capabilities.experimental = capabilities.experimental || {};
      capabilities.experimental.magit = {
        v: PROTOCOL_VERSION,
        minimumVersion: PROTOCOL_VERSION,
        keys: 'static',
        modifiers: 'full',
        layout: ['main'],
        effects: [...optionalEffects],
        prompts: [...optionalPromptKinds],
        pick: {
          multi: true,
          freeText: false,
          description: true,
          detail: true,
          group: false,
          preselection: true,
          fallbackView: true,
        },
      };
    },
    initialize() {},
    getState() { return { kind: 'static' }; },
    clear() {},
  });
  return client;
}

/**
 * Activate the extension.
 *
 * @param {vscode.ExtensionContext} context
 */
function activate(context) {
  store.output = vscode.window.createOutputChannel('magit-lsp');
  context.subscriptions.push(store.output);

  store.documentsChanged = new vscode.EventEmitter();
  context.subscriptions.push(store.documentsChanged);

  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(SCHEME, {
      onDidChange: store.documentsChanged.event,
      provideTextDocumentContent(uri) {
        const state = stateForUri(uri);
        return state ? state.lines.join('\n') : '';
      },
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('magit.open', openStatus),
    vscode.commands.registerCommand('magit.key', pressCommand),
     vscode.commands.registerCommand('magit.invoke', async (args) => {
       if (current()) return handleInvoke(args);
       return invokeCommand(args);
     }),
    vscode.commands.registerCommand('magit.confirmEdit', confirmEditCommand),
     vscode.commands.registerCommand('magit.cancelEdit', cancelEditCommand),
     vscode.commands.registerCommand('magit.closeEdit', () => {
       const draft = current();
       if (draft) return close(draft);
     }),
    vscode.commands.registerCommand('magit.mirrorFoldShortcuts', mirrorFoldShortcuts),
     vscode.workspace.onDidCloseTextDocument(onDocumentClosed),
     vscode.workspace.onDidChangeTextDocument(onChanged),
    vscode.window.onDidChangeActiveTextEditor(onActiveEditorChanged)
   );

   setup(context);

  store.clientReady = startClient(context).catch((error) => {
    store.output.appendLine(`language client failed: ${message(error)}`);
    vscode.window.showErrorMessage(`magit-lsp: ${message(error)}`);
    throw error;
  });
  // A user may never open a view; keep the failed readiness observable without
  // reporting it as an unhandled rejection.
  store.clientReady.catch(() => {});

  activateView(undefined);
}

/**
 * Resolve the server executable the client should start.
 *
 * A nonempty `magit-lsp.serverPath` always wins, so PATH names and absolute
 * paths are started exactly as configured and nothing is downloaded. Otherwise
 * the release pinned to this extension's manifest version is resolved below the
 * extension's own global storage.
 *
 * @param {vscode.ExtensionContext} context
 * @param {string} configured `magit-lsp.serverPath` setting
 * @returns {Promise<string>} launched command
 */
async function resolveServerCommand(context, configured) {
  if (configured) return configured;
  const resolved = await resolveServer({
    version: extensionVersion,
    cacheRoot: context.globalStorageUri?.fsPath,
  });
  return resolved.executable;
}

/**
 * Resolve the server, then construct, start and admit the language client.
 *
 * Resolution precedes construction so a failed download never constructs or
 * starts a transport, and version diagnostics name the executable that was
 * actually chosen.
 *
 * @param {vscode.ExtensionContext} context
 */
async function startClient(context) {
  const config = vscode.workspace.getConfiguration('magit-lsp');
  const profile = config.get('profile') || 'standard';
  const command = await resolveServerCommand(context, config.get('serverPath') || '');
  // Deactivation may have happened while a download was in flight; never start
  // a transport the editor is no longer able to stop.
  if (store.disposing) return;
  const executable = resolveCommand(command);
  const client = createClient(command, profile);
  store.client = client;
  client.onNotification('view/render', applyRender);
   client.onNotification('view/prompt', handlePrompt);
  client.onNotification('view/pick', handlePick);
   // The explicit full-sync stream excludes the draft scheme from the language
   // client's automatic document selector, avoiding duplicate native versions.
   if (client.onDidChangeState) context.subscriptions.push(client.onDidChangeState((event) => {
     if (event.newState === 1) disconnect('the server disconnected; copy your text before reopening');
   }));
  client.onNotification('view/effects', (params) => {
    void applyEffects(params && params.effects, store.activeViewId);
  });
  await client.start();
  const versionWarning = warning(extensionVersion,
    client.initializeResult?.serverInfo?.version, executable);
  if (versionWarning) vscode.window.showWarningMessage(versionWarning);
  const protocolFailure = protocolError(client.initializeResult, PROTOCOL_VERSION);
  if (protocolFailure) {
    const rejection = new Error(protocolFailure);
    try {
      await client.stop();
    } catch (cleanupError) {
      store.output.appendLine(`language client cleanup failed: ${message(cleanupError)}`);
    }
    throw rejection;
  }
  store.semanticChanged = register(
    vscode,
    client,
    stateForUri,
    context.subscriptions
  );
}

/**
 * Stop the language client when the extension deactivates.
 */
async function deactivate() {
  store.disposing = true;
  disconnect('the editor session is closing; copy your text before reopening');
  if (store.client) {
    await store.client.stop();
    store.client = undefined;
  }
}

exports.activate = activate;
exports.deactivate = deactivate;
