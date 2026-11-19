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
const { optionalEffects, optionalPromptKinds } = require('../types/shared-lists');
const { activateView, stateForUri } = require('./views');
const { SCHEME, store } = require('./store');
const { message } = require('./util');
const { resolveCommand, warning } = require('./version');
const extensionVersion = JSON.parse(fs.readFileSync(path.join(__dirname, '../package.json'), 'utf8')).version;

/**
 * Create the LSP client with the static key-delivery capabilities.
 *
 * @returns {LanguageClient}
 */
function createClient() {
  const config = vscode.workspace.getConfiguration('magit-lsp');
  const command = config.get('serverPath') || 'magit-lsp';
  const profile = config.get('profile') || 'standard';

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
        v: 1,
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

  store.client = createClient();
  const client = store.client;
  const command = vscode.workspace.getConfiguration('magit-lsp').get('serverPath') || 'magit-lsp';
  const executable = resolveCommand(command);
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
  store.clientReady = client.start().then(() => {
    const versionWarning = warning(extensionVersion,
      client.initializeResult?.serverInfo?.version, executable);
    if (versionWarning) vscode.window.showWarningMessage(versionWarning);
    store.semanticChanged = register(
      vscode,
      client,
      stateForUri,
      context.subscriptions
    );
  }).catch((error) => {
    store.output.appendLine(`language client failed: ${message(error)}`);
    vscode.window.showErrorMessage(`magit-lsp: ${message(error)}`);
  });

  activateView(undefined);
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
