"use strict";

const vscode = require("vscode");
const fs = require("node:fs");
const path = require("node:path");
const { LanguageClient } = require("vscode-languageclient/node");

const { openStatus, pressCommand, invokeCommand, mirrorFoldShortcuts } = require("./commands");
const {
  confirmEditCommand,
  cancelEditCommand,
  onDocumentClosed,
  onActiveEditorChanged,
} = require("./edits");
const { close, current, disconnect, handleInvoke, onChanged, setup } = require("./drafts");
const { applyEffects } = require("./effects");
const { applyRender, handlePrompt, handlePick } = require("./requests");
const { register } = require("./semantic");
const { resolveServerSource } = require("./server-source");
const { resolveServer } = require("./server-download");
const { optionalEffects, optionalPromptKinds } = require("../types/shared-lists");
const { activateView, stateForUri } = require("./views");
const { SCHEME, store } = require("./store");
const { message } = require("./util");
const { PROTOCOL_VERSION, protocolError, resolveCommand, warning } = require("./version");
const extensionVersion = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../package.json"), "utf8"),
).version;

/**
 * Create the LSP client with the static key-delivery capabilities.
 *
 * @param {string} command executable or PATH name the client starts
 * @param {string} profile keymap profile sent as an initialization option
 * @returns {LanguageClient}
 */
function createClient(command, profile) {
  const client = new LanguageClient(
    "magit-lsp",
    "magit-lsp",
    { command, args: [] },
    {
      documentSelector: [{ scheme: SCHEME }],
      initializationOptions: { profile },
      uriConverters: {
        code2Protocol: (uri) => stateForUri(uri)?.descriptor.uri || uri.toString(),
        protocol2Code: (uri) =>
          Array.from(store.views.values()).find((state) => state.descriptor.uri === uri)?.uri ||
          vscode.Uri.parse(uri),
      },
      middleware: {
        // The explicit provider maps dotted server types to legal VSCode IDs.
        provideDocumentSemanticTokens: () => null,
      },
    },
  );
  client.registerFeature({
    fillClientCapabilities(capabilities) {
      capabilities.experimental = capabilities.experimental || {};
      capabilities.experimental.magit = {
        v: PROTOCOL_VERSION,
        minimumVersion: PROTOCOL_VERSION,
        keys: "static",
        modifiers: "full",
        layout: ["main"],
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
    getState() {
      return { kind: "static" };
    },
    clear() {},
  });
  return client;
}

/**
 * Present a user-facing failure with exactly one extension prefix.
 *
 * Server resolution and download failures already carry the prefix; any other
 * failure is prefixed here, so no message is shown bare or doubled.
 *
 * @param {any} error a caught or thrown boundary value
 * @returns {string}
 */
function userFacing(error) {
  const text = message(error);
  return text.startsWith("magit-lsp: ") ? text : `magit-lsp: ${text}`;
}

/**
 * Describe the server serving this window, for the `magit.serverInfo` command.
 *
 * The version shown is the one the live handshake reported, so an explicit
 * override whose probe version is null still reports what the server itself
 * said. When no session completed its handshake the command reports that there
 * is no active server instead of failing.
 *
 * @returns {string} one user-facing line with a single product prefix
 */
function serverInfoCommand() {
  const server = store.server;
  if (!server || store.serverVersion === undefined) {
    return "magit-lsp: no active magit-lsp server";
  }
  const version = store.serverVersion || server.version;
  return version
    ? `magit-lsp: server ${version} (${server.origin}) at ${server.command}`
    : `magit-lsp: server (${server.origin}) at ${server.command}; version not reported`;
}

/**
 * Install or reinstall the managed server at the pinned extension version.
 *
 * The managed release is always the target, independent of `server.source`, and
 * a fresh download replaces any verified cache entry only after it succeeds, so
 * a failed reinstall leaves the previously installed binary working.
 *
 * @param {vscode.ExtensionContext} context
 * @returns {Promise<string>} one user-facing line with a single product prefix
 */
async function installManagedServer(context) {
  try {
    const resolved = await resolveServer({
      version: extensionVersion,
      cacheRoot: context.globalStorageUri?.fsPath,
      force: true,
    });
    const text = `magit-lsp: installed server ${resolved.version} at ${resolved.executable}`;
    vscode.window.showInformationMessage(text);
    return text;
  } catch (error) {
    const text = userFacing(error);
    vscode.window.showErrorMessage(text);
    return text;
  }
}

/**
 * Activate the extension.
 *
 * @param {vscode.ExtensionContext} context
 */
function activate(context) {
  store.output = vscode.window.createOutputChannel("magit-lsp");
  context.subscriptions.push(store.output);

  store.documentsChanged = new vscode.EventEmitter();
  context.subscriptions.push(store.documentsChanged);

  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(SCHEME, {
      onDidChange: store.documentsChanged.event,
      provideTextDocumentContent(uri) {
        const state = stateForUri(uri);
        return state ? state.lines.join("\n") : "";
      },
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("magit.open", openStatus),
    vscode.commands.registerCommand("magit.key", pressCommand),
    vscode.commands.registerCommand("magit.invoke", async (args) => {
      if (current()) return handleInvoke(args);
      return invokeCommand(args);
    }),
    vscode.commands.registerCommand("magit.confirmEdit", confirmEditCommand),
    vscode.commands.registerCommand("magit.cancelEdit", cancelEditCommand),
    vscode.commands.registerCommand("magit.closeEdit", () => {
      const draft = current();
      if (draft) return close(draft);
    }),
    vscode.commands.registerCommand("magit.mirrorFoldShortcuts", mirrorFoldShortcuts),
    vscode.commands.registerCommand("magit.serverInfo", () => {
      const text = serverInfoCommand();
      vscode.window.showInformationMessage(text);
      return text;
    }),
    vscode.commands.registerCommand("magit.installServer", () => installManagedServer(context)),
    vscode.workspace.onDidCloseTextDocument(onDocumentClosed),
    vscode.workspace.onDidChangeTextDocument(onChanged),
    vscode.window.onDidChangeActiveTextEditor(onActiveEditorChanged),
  );

  setup(context);

  store.clientReady = startClient(context).catch((error) => {
    store.output.appendLine(`language client failed: ${message(error)}`);
    vscode.window.showErrorMessage(userFacing(error));
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
 * `magit-lsp.server.path` is the explicit override and `magit-lsp.server.source`
 * selects the automatic order (explicit path, matching PATH binary, managed
 * download), a PATH-only system binary that never downloads, or a forced
 * managed download. A failed or incompatible explicit path is reported, never
 * silently replaced. The resolved origin and version are kept for diagnostics.
 *
 * @param {vscode.ExtensionContext} context
 * @param {{get: (name: string) => any}} config `magit-lsp` configuration
 * @returns {Promise<string>} launched command
 */
async function resolveServerCommand(context, config) {
  const resolved = await resolveServerSource({
    source: config.get("server.source"),
    path: config.get("server.path"),
    version: extensionVersion,
    cacheRoot: context.globalStorageUri?.fsPath,
  });
  store.server = resolved;
  return resolved.command;
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
  const config = vscode.workspace.getConfiguration("magit-lsp");
  const profile = config.get("profile") || "standard";
  const command = await resolveServerCommand(context, config);
  // Deactivation may have happened while a download was in flight; never start
  // a transport the editor is no longer able to stop.
  if (store.disposing) return;
  const executable = resolveCommand(command);
  const client = createClient(command, profile);
  store.client = client;
  client.onNotification("view/render", applyRender);
  client.onNotification("view/prompt", handlePrompt);
  client.onNotification("view/pick", handlePick);
  // The explicit full-sync stream excludes the draft scheme from the language
  // client's automatic document selector, avoiding duplicate native versions.
  if (client.onDidChangeState)
    context.subscriptions.push(
      client.onDidChangeState((event) => {
        if (event.newState === 1)
          disconnect("the server disconnected; copy your text before reopening");
      }),
    );
  client.onNotification("view/effects", (params) => {
    void applyEffects(params && params.effects, store.activeViewId);
  });
  await client.start();
  const versionWarning = warning(
    extensionVersion,
    client.initializeResult?.serverInfo?.version,
    executable,
  );
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
  store.serverVersion = client.initializeResult?.serverInfo?.version ?? null;
  store.semanticChanged = register(vscode, client, stateForUri, context.subscriptions);
}

/**
 * Stop the language client when the extension deactivates.
 */
async function deactivate() {
  store.disposing = true;
  store.serverVersion = undefined;
  disconnect("the editor session is closing; copy your text before reopening");
  if (store.client) {
    await store.client.stop();
    store.client = undefined;
  }
}

exports.activate = activate;
exports.deactivate = deactivate;
