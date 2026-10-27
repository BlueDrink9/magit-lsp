'use strict';

const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

const commands = require('./commands');
const edits = require('./edits');
const drafts = require('./drafts');
const effects = require('./effects');
const requests = require('./requests');
const semantic = require('./semantic');
const views = require('./views');
const { SCHEME, store } = require('./store');
const { message } = require('./util');

/**
 * Create the LSP client with the static key-delivery capabilities.
 *
 * @returns {LanguageClient}
 */
function createClient() {
  const config = vscode.workspace.getConfiguration('magit-lsp');
  const command = config.get('serverPath') || 'magit-lsp';
  const profile = config.get('profile') || 'standard';

  return new LanguageClient(
    'magit-lsp',
    'magit-lsp',
    { command, args: [] },
    {
      documentSelector: [{ scheme: SCHEME }],
      initializationOptions: { profile },
      uriConverters: {
        code2Protocol: (uri) => views.stateForUri(uri)?.descriptor.uri || uri.toString(),
        protocol2Code: (uri) => Array.from(store.views.values())
          .find((state) => state.descriptor.uri === uri)?.uri || vscode.Uri.parse(uri),
      },
      middleware: {
        // The explicit provider maps dotted server types to legal VSCode IDs.
        provideDocumentSemanticTokens: () => null,
        initialize(params, second, third) {
          params.capabilities = params.capabilities || {};
          params.capabilities.keymapDelivery = 'static';
          params.capabilities.keyModifiers = 'full';
          const next = typeof second === 'function' ? second : third;
          return next(params);
        },
      },
    }
  );
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
        const state = views.stateForUri(uri);
        return state ? state.lines.join('\n') : '';
      },
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('magit.open', commands.openStatus),
    vscode.commands.registerCommand('magit.key', commands.pressCommand),
     vscode.commands.registerCommand('magit.invoke', async (args) => {
       if (drafts.current()) return drafts.handleInvoke(args);
       return commands.invokeCommand(args);
     }),
    vscode.commands.registerCommand('magit.confirmEdit', edits.confirmEditCommand),
     vscode.commands.registerCommand('magit.cancelEdit', edits.cancelEditCommand),
     vscode.commands.registerCommand('magit.closeEdit', () => {
       const draft = drafts.current();
       if (draft) return drafts.close(draft);
     }),
     vscode.commands.registerCommand('magit.historyPrev', () => {
       const draft = drafts.current();
       if (draft) return drafts.history(draft, 'commit-message.history-prev');
     }),
     vscode.commands.registerCommand('magit.historyNext', () => {
       const draft = drafts.current();
       if (draft) return drafts.history(draft, 'commit-message.history-next');
     }),
    vscode.commands.registerCommand('magit.mirrorFoldShortcuts', commands.mirrorFoldShortcuts),
     vscode.workspace.onDidCloseTextDocument(edits.onDocumentClosed),
     vscode.workspace.onDidChangeTextDocument(drafts.onChanged),
    vscode.window.onDidChangeActiveTextEditor(edits.onActiveEditorChanged)
   );

   drafts.setup(context);

  store.client = createClient();
  const client = store.client;
  client.onNotification('view/render', requests.applyRender);
   client.onNotification('view/prompt', requests.handlePrompt);
   client.onNotification('draft/error', drafts.onError);
   // The explicit full-sync stream excludes the draft scheme from the language
   // client's automatic document selector, avoiding duplicate native versions.
   if (client.onDidChangeState) context.subscriptions.push(client.onDidChangeState((event) => {
     if (event.newState === 1) drafts.disconnect('the server disconnected; copy your text before reopening');
   }));
  client.onNotification('view/effects', (params) => {
    void effects.applyEffects(params && params.effects, store.activeViewId);
  });
  store.clientReady = client.start().then(() => {
    store.semanticChanged = semantic.register(
      vscode,
      client,
      views.stateForUri,
      context.subscriptions
    );
  }).catch((error) => {
    store.output.appendLine(`language client failed: ${message(error)}`);
    vscode.window.showErrorMessage(`magit-lsp: ${message(error)}`);
  });

  views.activateView(undefined);
}

/**
 * Stop the language client when the extension deactivates.
 */
async function deactivate() {
  store.disposing = true;
  drafts.disconnect('the editor session is closing; copy your text before reopening');
  if (store.client) {
    await store.client.stop();
    store.client = undefined;
  }
}

exports.activate = activate;
exports.deactivate = deactivate;
exports.createClient = createClient;
