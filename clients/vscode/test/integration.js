"use strict";

/**
 * Node integration test: activate the real extension against the real server.
 *
 * A scripted `vscode` module supplies the editor widgets and virtual
 * documents; the fake `LanguageClient` speaks LSP to `target/debug/magit-lsp`
 * over stdio. The test proves the thin-client contract end to end: open a view,
 * render the server's text verbatim, press a key that triggers a render, use a
 * menu QuickPick, and answer a server prompt.
 */

const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const { constants } = fs;
const os = require("node:os");
const path = require("node:path");
const Module = require("node:module");

const vscodeModule = require("./fake/vscode");
const { Position, Selection, __test, commands: vscodeCommands, window, workspace } = vscodeModule;
const { LanguageClient, instances } = require("./fake/languageclient");

/** Scripted client with explicit, validated work-done progress support. */
class IntegrationClient extends LanguageClient {
  constructor(id, name, serverOptions, clientOptions) {
    super(id, name, serverOptions, {
      ...clientOptions,
    });
    this.registerFeature({
      fillClientCapabilities(capabilities) {
        capabilities.window = { workDoneProgress: true };
      },
      initialize() {},
      getState() {
        return { kind: "static" };
      },
      clear() {},
    });
    this.createdProgress = new Set();
    this.progress = new Map();
    this.onNotification("$/progress", ({ token, value }) => {
      assert(this.createdProgress.has(token), "progress must be created before use");
      if (value.kind === "begin") {
        assert(!this.progress.has(token), "progress begins once");
      } else {
        assert.equal(this.progress.get(token), "begin", "progress must be active");
      }
      this.progress.set(token, value.kind === "report" ? "begin" : value.kind);
    });
  }

  /** Acknowledge and track actual server progress-create requests. */
  dispatch(message) {
    if (message.method === "window/workDoneProgress/create" && message.id !== undefined) {
      const { token } = message.params;
      assert(!this.createdProgress.has(token), "progress tokens are unique");
      this.createdProgress.add(token);
      this.write({ jsonrpc: "2.0", id: message.id, result: null });
      return;
    }
    super.dispatch(message);
  }
}

// Model the native workbench close command without weakening production code:
// invoke the registered tab observer only after the tab is absent from every
// group. Dirty choices use the existing scripted answer solely in this fake.
const tabObserver = { handler: undefined };
const nativeTabs = new WeakMap();
Object.defineProperty(window.tabGroups, "all", {
  get() {
    return __test.editors.map((editor) => {
      if (!nativeTabs.has(editor)) nativeTabs.set(editor, { input: { uri: editor.document.uri } });
      return { tabs: [nativeTabs.get(editor)] };
    });
  },
});
window.tabGroups.onDidChangeTabs = (handler) => {
  tabObserver.handler = handler;
  return { dispose() {} };
};
const executeWorkbenchCommand = vscodeCommands.executeCommand.bind(vscodeCommands);
vscodeCommands.executeCommand = async (name, ...args) => {
  if (name !== "workbench.action.closeActiveEditor") return executeWorkbenchCommand(name, ...args);
  const draft = require("../lib/drafts").current();
  if (!draft) return;
  if (draft.document.isDirty) {
    const selected = await window.showQuickPick([
      { id: "save" },
      { id: "discard" },
      { id: "cancel" },
    ]);
    if (!selected || selected.id === "cancel") return;
    if (selected.id === "save") await draft.document.save();
    else await executeWorkbenchCommand("workbench.action.files.revert", draft.uri);
  }
  const tab = window.tabGroups.all
    .flatMap((group) => group.tabs)
    .find((item) => item.input.uri.toString() === draft.uri.toString());
  __test.editors.splice(
    0,
    __test.editors.length,
    ...__test.editors.filter((editor) => editor.document !== draft.document),
  );
  draft.document.isClosed = true;
  if (tabObserver.handler) tabObserver.handler({ closed: [tab] });
};

// Model VSCode's built-in local filesystem, not an extension draft provider.
const savedHandlers = [];
workspace.onDidSaveTextDocument = (handler) => {
  savedHandlers.push(handler);
  return { dispose() {} };
};
const nativeOpen = workspace.openTextDocument;
workspace.openTextDocument = async (...args) => {
  const document = await nativeOpen(...args);
  if (!document.nativeSaveInstalled) {
    document.nativeSaveInstalled = true;
    const nativeSave = document.save.bind(document);
    document.save = async () => {
      const saved = await nativeSave();
      for (const handler of savedHandlers) handler(document);
      const record = require("../lib/drafts").forUri(document.uri);
      if (record && !record.busy) await record.queue;
      return saved;
    };
  }
  return document;
};

/** Test root: `clients/vscode/test`. */
const here = __dirname;

/**
 * Resolve after `milliseconds`; used only between observation polls.
 *
 * @param {number} milliseconds
 * @returns {Promise<void>}
 */
function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/**
 * Poll `condition` until it is truthy or the timeout expires.
 *
 * @param {() => unknown} condition
 * @param {string} label
 * @param {number} [timeout]
 * @returns {Promise<void>}
 */
async function waitFor(condition, label, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (!condition()) {
    if (Date.now() > deadline) {
      throw new Error(`timed out waiting for ${label}`);
    }
    await delay(20);
  }
}

/**
 * Run Git in the fixture repository.
 *
 * @param {string} repo
 * @param {string[]} args
 */
function git(repo, args) {
  execFileSync("git", ["-C", repo, ...args], { stdio: "ignore" });
}

/**
 * Create a repository with one modified tracked file.
 *
 * @returns {string} repository path
 */
function createRepository() {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), "magit-lsp-vscode-"));
  git(repo, ["init", "-q"]);
  git(repo, ["config", "user.name", "Test"]);
  git(repo, ["config", "user.email", "test@example.com"]);
  fs.writeFileSync(path.join(repo, "tracked.txt"), "original\n");
  git(repo, ["add", "."]);
  git(repo, ["commit", "-qm", "initial"]);
  fs.writeFileSync(path.join(repo, "tracked.txt"), "changed\n");
  return repo;
}

/**
 * Hex SHA-256 of bytes.
 *
 * @param {Buffer} bytes
 * @returns {string}
 */
function digest(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

/**
 * Build a release archive around the real server binary with the packager's
 * Python tarfile settings.
 *
 * @param {string} binary path to `magit-lsp`
 * @param {string} stem versioned archive directory
 * @param {string} archiveName published archive file name
 * @returns {Buffer} gzip release archive bytes
 */
function packagedArchive(binary, stem, archiveName) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "magit-lsp-integration-archive-"));
  try {
    const binaryPath = path.join(directory, "magit-lsp");
    fs.copyFileSync(binary, binaryPath);
    const archivePath = path.join(directory, archiveName);
    execFileSync(
      "python3",
      [
        path.resolve(here, "../../../scripts/test_release_archive_fixture.py"),
        archivePath,
        stem,
        binaryPath,
      ],
      { stdio: "pipe" },
    );
    return fs.readFileSync(archivePath);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

/**
 * Open the status view with the default empty server path.
 *
 * The cache is populated by the real download helper from a fixture release
 * archive containing the real server binary, so resolution, launch and the
 * initialize handshake are exercised without any network access.
 *
 * @param {string} binary path to `magit-lsp`
 * @returns {Promise<void>}
 */
async function automaticStartup(binary) {
  const version = require("../package.json").version;
  const asset = "x86_64-unknown-linux-gnu";
  const stem = `magit-lsp-v${version}-${asset}`;
  const archiveName = `${stem}.tar.gz`;
  const base = `https://github.com/BlueDrink9/magit-lsp/releases/download/v${version}`;
  const cacheRoot = fs.mkdtempSync(path.join(os.tmpdir(), "magit-lsp-vscode-storage-"));
  const repo = createRepository();
  try {
    const archive = packagedArchive(binary, stem, archiveName);
    const { resolveServer } = require("../lib/server-download");
    const resolved = await resolveServer({
      version,
      cacheRoot,
      runtime: { platform: "linux", arch: "x64", glibc: "2.36" },
      transport: async (url) => {
        if (url === `${base}/${archiveName}.sha256`) {
          return Buffer.from(`${digest(archive)}  ${archiveName}\n`);
        }
        if (url === `${base}/${archiveName}`) return archive;
        throw new Error(`unexpected download ${url}`);
      },
    });

    __test.reset();
    workspace.registerFileSystemProvider("file", {
      readFile: (uri) => fs.readFileSync(uri.fsPath),
      writeFile: (uri, bytes) => fs.writeFileSync(uri.fsPath, bytes),
    });
    __test.setWorkspaceFolder(repo);
    __test.setConfiguration({
      "magit-lsp.profile": "standard",
      "magit-lsp.server.source": "managed",
    });
    assert.equal(
      workspace.getConfiguration("magit-lsp").get("server.path"),
      undefined,
      "the automatic startup leaves the server path unset",
    );

    const extension = loadExtension();
    extension.activate({
      subscriptions: [],
      globalStorageUri: vscodeModule.Uri.file(cacheRoot),
    });
    let client;
    try {
      await require("../lib/store").store.clientReady;
      client = instances.at(-1);
      assert(client, "the extension started a language client");
      assert.equal(
        client.serverOptions.command,
        resolved.executable,
        "the resolved release executable is launched",
      );
      assert.equal(
        resolved.executable.startsWith(cacheRoot),
        true,
        "the executable is installed in extension global storage",
      );
      await __test.commands.get("magit.open")();
      await waitFor(
        () =>
          __test.activeEditor && /Unstaged changes/.test(__test.activeEditor.document.getText()),
        "status view from the downloaded server",
      );
      assert.equal(
        client.initializeResult.capabilities.experimental.magit.protocolVersion,
        1,
        "the downloaded server completes the matching handshake",
      );
      assert.deepEqual(
        __test.messages.filter((notice) => notice.level === "error"),
        [],
        "automatic startup reports no error",
      );
    } finally {
      await extension.deactivate();
    }
    const exited = await Promise.race([
      client.exited.then(() => true),
      delay(5000).then(() => false),
    ]);
    assert(exited, "the downloaded language server exited after deactivate");
  } finally {
    fs.rmSync(cacheRoot, { recursive: true, force: true });
    fs.rmSync(repo, { recursive: true, force: true });
  }
}

/**
 * Load `extension.js` with the fake editor modules installed.
 *
 * Evicting the extension modules lets this test activate twice: once with the
 * downloaded release and once with an explicitly configured server path.
 *
 * @returns {object} the extension module
 */
function loadExtension() {
  const root = path.join(__dirname, "..");
  const owned = [path.join(root, "extension.js"), path.join(root, "package.json")];
  const prefixes = [path.join(root, "lib") + path.sep, path.join(root, "types") + path.sep];
  for (const key of Object.keys(require.cache)) {
    if (owned.includes(key) || prefixes.some((prefix) => key.startsWith(prefix))) {
      delete require.cache[key];
    }
  }
  const originalLoad = Module._load;
  Module._load = function load(request, parent, isMain) {
    if (request === "vscode") return vscodeModule;
    if (request === "vscode-languageclient/node") return { LanguageClient: IntegrationClient };
    return originalLoad.call(this, request, parent, isMain);
  };
  try {
    return require("../extension");
  } finally {
    Module._load = originalLoad;
  }
}

/** Run the scenario, reporting product output when an assertion fails. */
async function run() {
  const defaultBinary = path.resolve(
    here,
    "../../../target/debug",
    `magit-lsp${process.platform === "win32" ? ".exe" : ""}`,
  );
  function validateBinary(binary) {
    if (!path.isAbsolute(binary)) throw new Error("MAGIT_LSP_SERVER_PATH must be absolute");
    try {
      fs.accessSync(binary, constants.X_OK);
    } catch {
      throw new Error(`MAGIT_LSP_SERVER_PATH must be an executable file: ${binary}`);
    }
    return binary;
  }
  assert.throws(() => validateBinary("relative/server"), /must be absolute/);
  assert.throws(
    () => validateBinary(path.join(os.tmpdir(), "missing-mag it-lsp")),
    /executable file/,
  );
  const spacedDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "magit lsp selected server "));
  const spacedBinary = path.join(
    spacedDirectory,
    `server with spaces${process.platform === "win32" ? ".exe" : ""}`,
  );
  fs.symlinkSync(defaultBinary, spacedBinary);
  assert.equal(
    validateBinary(spacedBinary),
    spacedBinary,
    "absolute executable paths with spaces are accepted",
  );
  fs.rmSync(spacedDirectory, { recursive: true, force: true });
  const requestedBinary = process.env.MAGIT_LSP_SERVER_PATH;
  let binary = requestedBinary ? validateBinary(requestedBinary) : defaultBinary;
  if (!fs.existsSync(binary)) {
    throw new Error(`missing ${binary}; run \`cargo build\` or \`make build\` first`);
  }
  if (!requestedBinary) binary = validateBinary(binary);
  assert.equal(
    binary,
    requestedBinary || defaultBinary,
    "the selected absolute server path is retained",
  );

  await automaticStartup(binary);

  const repo = createRepository();
  __test.reset();
  workspace.registerFileSystemProvider("file", {
    readFile: (uri) => fs.readFileSync(uri.fsPath),
    writeFile: (uri, bytes) => fs.writeFileSync(uri.fsPath, bytes),
  });
  __test.setWorkspaceFolder(repo);
  __test.setConfiguration({
    "magit-lsp.server.path": binary,
    "magit-lsp.profile": "standard",
  });

  const extension = loadExtension();
  const context = { subscriptions: [] };
  extension.activate(context);

  try {
    const commands = __test.commands;
    await require("../lib/store").store.clientReady;
    const client = instances.at(-1);
    assert(client, "the extension started a language client");
    const declaration = client.requests.find((request) => request.method === "initialize").params
      .capabilities.experimental.magit;
    const { optionalEffects, optionalPromptKinds } = require("../types/shared-lists");
    assert.deepEqual(declaration, {
      v: 1,
      minimumVersion: 1,
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
    });
    assert.equal(Object.hasOwn(declaration, "views"), false);

    // 1. Open the status view and display the server's text verbatim.
    await commands.get("magit.open")();
    assert.equal(
      client.initializeResult.capabilities.experimental.magit.protocolVersion,
      1,
      "server advertises the declared rich protocol version",
    );
    await waitFor(
      () => __test.activeEditor && /Unstaged changes/.test(__test.activeEditor.document.getText()),
      "status view text",
    );
    const statusEditor = __test.activeEditor;
    const openedText = statusEditor.document.getText();
    assert.match(openedText, /Unstaged changes/);
    assert.equal(statusEditor.document.uri.scheme, "magit");
    assert(
      client.requests.some((request) => request.method === "view/open"),
      "opened the view through view/open",
    );
    assert(
      client.notes.some((note) => note.method === "view/rendered"),
      "acknowledged the first render",
    );

    // 2. A key press goes to the server and its render replaces the text.
    fs.writeFileSync(path.join(repo, "added.txt"), "new\n");
    const rendersBefore = client.notes.filter((note) => note.method === "view/rendered").length;
    await commands.get("magit.key")({ key: "g" });
    await waitFor(
      () => /added\.txt/.test(statusEditor.document.getText()),
      "refreshed status text",
    );
    assert(
      client.requests.some(
        (request) =>
          request.method === "view/key" &&
          request.params.key === "g" &&
          request.params.viewId === "v1",
      ),
      "forwarded the canonical key with the server view id",
    );
    assert(
      client.notes.filter((note) => note.method === "view/rendered").length > rendersBefore,
      "acknowledged the render caused by the key press",
    );

    await require("./selection-integration").checkShapes({
      vscode: vscodeModule,
      commands,
      client,
      editor: statusEditor,
    });

    // Static mode is a metadata-only transition: it must not invalidate the
    // virtual document or disturb the user's selection.
    const statusState = require("../lib/store").store.views.get("v1");
    const selection = new Selection(new Position(0, 0), new Position(0, 3));
    statusEditor.selection = selection;
    const linesBeforeStatic = statusState.lines;
    const versionBeforeStatic = statusState.version;
    let documentsChangedCount = 0;
    const documentsChangedSubscription = require("../lib/store").store.documentsChanged.event(
      () => {
        documentsChangedCount += 1;
      },
    );
    const changedBeforeStatic = client.notes.filter(
      (note) => note.method === "view/rendered",
    ).length;
    await commands.get("magit.invoke")({ command: "common.toggle-text-mode" });
    await waitFor(() => __test.contextValues.get("magit.textMode") === true, "text-mode context");
    assert.equal(__test.contextValues.get("magit.textMode"), true);
    assert.equal(statusEditor.selection, selection);
    assert.equal(statusState.lines, linesBeforeStatic);
    assert.equal(statusState.version, versionBeforeStatic);
    assert.equal(documentsChangedCount, 0);
    assert.equal(
      client.notes.filter((note) => note.method === "view/rendered").length,
      changedBeforeStatic + 1,
    );
    assert.equal(client.notes.at(-1).method, "view/rendered");
    assert.equal(client.notes.at(-1).params.version, versionBeforeStatic);

    // Refreshes queued while static are applied as a full server render on
    // return to interactive mode.
    fs.writeFileSync(path.join(repo, "queued.txt"), "queued\n");
    await commands.get("magit.key")({ key: "g" });
    assert.equal(statusState.lines, linesBeforeStatic);
    assert.equal(statusEditor.selection, selection);
    await commands.get("magit.invoke")({ command: "common.toggle-text-mode" });
    await waitFor(
      () => __test.contextValues.get("magit.textMode") === false,
      "interactive-mode context",
    );
    await waitFor(
      () => /queued\.txt/.test(statusEditor.document.getText()),
      "queued refresh after static mode",
    );
    assert.equal(__test.contextValues.get("magit.textMode"), false);
    assert.notEqual(statusState.lines, linesBeforeStatic);
    documentsChangedSubscription.dispose();

    // 3. A server menu becomes a QuickPick; selecting a row sends its key.
    let promptItems;
    __test.setQuickPickAnswer((items) => {
      promptItems = items;
      return items.find((item) => item.id === "no");
    });
    await commands.get("magit.invoke")({ command: "open.commit" });
    await waitFor(
      () => __test.lastQuickPick && __test.lastQuickPick.items.some((item) => item.key === "c"),
      "commit menu QuickPick",
    );
    const menu = __test.lastQuickPick;
    const initialMenuState = require("../lib/store").store.views.get(
      require("../lib/store").store.quickPickViewId,
    );
    const { applyEffects } = require("../lib/effects");
    initialMenuState.rootId = undefined;
    await applyEffects([{ type: "pushView", descriptor: initialMenuState.descriptor }], "v1");
    assert.equal(
      initialMenuState.rootId,
      statusState.rootId,
      "PushView inherits the originating root",
    );
    assert.match(menu.title, /Commit/);
    menu.select(menu.items.find((item) => item.key === "c"));

    // 4. The menu action prompts, and the scripted answer reaches the server.
    await waitFor(
      () => client.requests.some((request) => request.method === "view/promptRespond"),
      "prompt response",
    );
    const answer = client.requests.find(
      (request) => request.method === "view/promptRespond",
    ).params;
    assert.equal(answer.cancelled, false);
    assert.equal(answer.value, false, "answered the confirm prompt with No");
    assert(promptItems, "the prompt offered choices through showQuickPick");
    assert.deepEqual(
      promptItems.map((item) => item.id),
      ["yes", "no"],
    );
    assert(
      client.requests.some(
        (request) => request.method === "view/key" && request.params.key === "c",
      ),
      "the menu row was forwarded as a canonical key, not a client command",
    );

    // 4b. A server pick becomes a QuickPick and answers with item ids.
    const pickParams = {
      pickId: "pick-integration",
      purpose: "select-revision",
      prompt: "Select a revision",
      multi: false,
      allowFreeText: false,
      preselection: [],
      items: [
        { id: "a", label: "alpha" },
        { id: "b", label: "beta" },
      ],
    };
    let pickItems;
    __test.setQuickPickAnswer((items) => {
      pickItems = items;
      return items[1];
    });
    client.dispatch({ jsonrpc: "2.0", method: "view/pick", params: pickParams });
    await waitFor(
      () =>
        client.requests.some(
          (request) =>
            request.method === "view/pickRespond" && request.params.pickId === "pick-integration",
        ),
      "pick response",
    );
    assert.deepEqual(
      client.requests.find(
        (request) =>
          request.method === "view/pickRespond" && request.params.pickId === "pick-integration",
      ).params,
      { pickId: "pick-integration", cancelled: false, selected: ["b"] },
    );
    assert(pickItems, "the pick offered items through showQuickPick");
    assert.deepEqual(
      pickItems.map((item) => item.id),
      ["a", "b"],
    );

    // Cancelling a pick answers an empty selection.
    __test.setQuickPickAnswer(() => undefined);
    const cancelPickId = "pick-cancel-integration";
    client.dispatch({
      jsonrpc: "2.0",
      method: "view/pick",
      params: { ...pickParams, pickId: cancelPickId },
    });
    await waitFor(
      () =>
        client.requests.some(
          (request) =>
            request.method === "view/pickRespond" && request.params.pickId === cancelPickId,
        ),
      "pick cancel response",
    );
    assert.deepEqual(
      client.requests.find(
        (request) =>
          request.method === "view/pickRespond" && request.params.pickId === cancelPickId,
      ).params,
      { pickId: cancelPickId, cancelled: true, selected: [] },
    );

    // Exercise real showMenu accept handlers on live menu views. Synthetic
    // structured rows cover canonical shapes absent from the standard profile;
    // their keys still travel through requests.press to the real server.
    const { updateQuickPick } = require("../lib/menus");
    const viewStore = require("../lib/store").store;
    await initialMenuState.pressQueue;
    const keyRequestsSince = (start, id) =>
      client.requests
        .slice(start)
        .filter((request) => request.method === "view/key" && request.params.viewId === id)
        .map((request) => request.params.key);
    const activationCases = [
      ["-a", ["-", "a"]],
      ["=A", ["=", "A"]],
      ["g l", ["g", "l"]],
      ["ctrl+t", ["ctrl+t"]],
      ["Enter", ["Enter"]],
      ["G", ["G"]],
      ["Plus", ["Plus"]],
    ];
    for (const [key, activationSteps] of activationCases) {
      await commands.get("magit.invoke")({ command: "open.commit" });
      await waitFor(
        () => viewStore.views.get(viewStore.quickPickViewId)?.quickPick,
        `live menu for ${key}`,
      );
      const state = viewStore.views.get(viewStore.quickPickViewId);
      assert(state?.quickPick, `live showMenu widget for ${key}`);
      state.menu = {
        ...state.menu,
        groups: [
          {
            title: "Activation regression",
            items: [
              {
                kind: "action",
                key,
                activationSteps,
                label: "Planned activation",
                status: { status: "planned", roadmapId: "R-4.1" },
              },
            ],
          },
        ],
      };
      updateQuickPick(state);
      const row = state.quickPick.items.find((item) => item.key === key);
      assert.equal(row.status, "planned");
      assert.deepEqual(row.activationSteps, activationSteps);
      const start = client.requests.length;
      state.quickPick.select(row);
      await state.pressQueue;
      assert.deepEqual(keyRequestsSince(start, state.id), activationSteps, key);
      state.quickPick.value = "q";
      await state.pressQueue;
      assert.equal(viewStore.views.has(state.id), false, "real server close applied");
    }

    await commands.get("magit.invoke")({ command: "open.commit" });
    await waitFor(
      () => viewStore.views.get(viewStore.quickPickViewId)?.quickPick,
      "live typed-input menu",
    );
    const typedState = viewStore.views.get(viewStore.quickPickViewId);
    const typedStart = client.requests.length;
    typedState.quickPick.value = "g l+G";
    await typedState.pressQueue;
    assert.deepEqual(keyRequestsSince(typedStart, typedState.id), ["g", "Space", "l", "Plus", "G"]);
    assert.equal(typedState.quickPick.value, "", "typed input is still cleared");

    // Acceptance queues stop after an actual server close, including later
    // accept events queued before that close has arrived.
    typedState.menu = {
      ...typedState.menu,
      groups: [
        {
          title: "Close regression",
          items: [
            {
              kind: "action",
              key: "q x",
              activationSteps: ["q", "x"],
              label: "Close then tail",
              status: { status: "implemented" },
            },
          ],
        },
      ],
    };
    updateQuickPick(typedState);
    const closeStart = client.requests.length;
    const closingPick = typedState.quickPick;
    closingPick.select(closingPick.items.find((item) => item.key === "q x"));
    closingPick.select(closingPick.items.find((item) => item.key === "q x"));
    await typedState.pressQueue;
    assert.deepEqual(keyRequestsSince(closeStart, typedState.id), ["q"]);
    assert.equal(viewStore.views.has(typedState.id), false);
    await applyEffects([{ type: "popView", viewId: typedState.id }], "v1");
    assert.equal(
      viewStore.views.has(typedState.id),
      false,
      "PopView leaves the closed child absent",
    );

    // The real prompt handler must interpret the first displayed row as No
    // when the server's default is false (including destructive prompts).
    const { promptFor } = require("../lib/requests");
    __test.setQuickPickAnswer((items) => items[0]);
    for (const vector of require("../../../tests/data/client-contracts.json").prompts.filter(
      (item) => item.name === "confirm-default-false" || item.name === "confirm-destructive",
    )) {
      const result = await promptFor(vector.params);
      assert.deepEqual(result, vector.answer, vector.name);
    }

    // Exercise the actual editor bridge through the registered dispatcher and
    // generated binding payloads rather than simulating profile key chords.
    const invokeGenerated = async (command) => {
      const binding = require("../package.json").contributes.keybindings.find(
        (entry) =>
          entry.args?.command === command && entry.when.includes("magit.scope == 'commit-message'"),
      );
      assert(binding, `generated binding for ${command}`);
      await commands.get(binding.command)(binding.args);
    };
    const { store: draftStore } = require("../lib/store");
    const progress = client.progress;
    assert.equal(
      client.requests.find((request) => request.method === "initialize").params.capabilities.window
        .workDoneProgress,
      true,
    );
    const draftProgress = new WeakMap();
    const head = () => execFileSync("git", ["-C", repo, "rev-parse", "HEAD"]).toString();
    const subject = () =>
      execFileSync("git", ["-C", repo, "log", "-1", "--format=%s"]).toString().trim();
    const idle = async (draft) => {
      const token = draftProgress.get(draft);
      assert.notEqual(token, undefined, "draft must have observed progress begin");
      await draft.queue;
      await waitFor(() => progress.get(token) === "end", "commit progress end");
    };
    let probe = 0;
    let previousDraft;
    const startDraft = async () => {
      if (previousDraft) await idle(previousDraft);
      await window.showTextDocument(statusEditor.document);
      fs.writeFileSync(path.join(repo, "tracked.txt"), `managed probe ${++probe}\n`);
      git(repo, ["add", "tracked.txt"]);
      const before = new Set(draftStore.drafts.keys());
      const beforeProgress = new Set(progress.keys());
      await commands.get("magit.invoke")({ command: "commit.create" });
      await waitFor(
        () =>
          [...draftStore.drafts.values()].some(
            (draft) => !before.has(draft.runId) && draft.attached,
          ),
        "attached managed draft",
      );
      const draft = [...draftStore.drafts.values()].find((record) => !before.has(record.runId));
      await waitFor(
        () => [...progress].some(([token, kind]) => !beforeProgress.has(token) && kind === "begin"),
        "commit progress begin",
      );
      const active = [...progress].filter(
        ([token, kind]) => !beforeProgress.has(token) && kind === "begin",
      );
      assert.equal(active.length, 1, "one active progress token for this commit");
      draftProgress.set(draft, active[0][0]);
      previousDraft = draft;
      assert.equal(draft.document.uri.scheme, "file");
      assert.equal(draft.document.languageId, "git-commit");
      const opens = client.notes.filter(
        (note) =>
          note.method === "textDocument/didOpen" &&
          note.params.textDocument.uri === draft.effect.uri,
      );
      assert.equal(opens.length, probe);
      assert.deepEqual(opens.at(-1).params.textDocument, {
        uri: draft.effect.uri,
        languageId: "gitcommit",
        version: 1,
        text: draft.effect.text,
      });
      assert.equal(draft.document.getText(), draft.effect.text);
      assert.equal(
        __test.contextValues.get("magit.active"),
        false,
        "native typing is not captured",
      );
      return draft;
    };
    const unchangedIndex = () => execFileSync("git", ["-C", repo, "show", ":tracked.txt"]);
    const deferredAbort = () => {
      let release;
      const response = new Promise((resolve) => {
        release = resolve;
      });
      return { response, release };
    };
    let draft = await startDraft();
    const commitMessage = `integration managed ${Date.now()}`;
    const beforeSave = head();
    draft.document.replaceText(`${commitMessage}\n`);
    assert(await draft.document.save());
    assert.equal(head(), beforeSave, "save only checkpoints");
    assert(draftStore.drafts.has(draft.runId), "save stays editing");
    const savedVersion = draft.savedVersion;
    await commands.get("magit.closeEdit")();
    await idle(draft);
    assert.equal(subject(), commitMessage, "normal close commits saved checkpoint");
    assert(
      client.requests.some(
        (request) =>
          request.method === "workspace/executeCommand" &&
          request.params.arguments[0].version === savedVersion &&
          request.params.arguments[0].source === "saved-file",
      ),
    );

    draft = await startDraft();
    const unsent = "exact unsent message\n\nbody ü\n";
    draft.document.replaceText(unsent);
    await waitFor(() => draft.text === unsent && !draft.flushQueued, "synchronized native text");
    const recoveryVersion = draft.version;
    await require("../lib/drafts").open(
      {
        ...draft.effect,
        version: recoveryVersion,
        text: "older server snapshot must not replace local edits",
      },
      draft.baseViewId,
    );
    assert.equal(draft.document.getText(), unsent, "same-run presentation preserves native text");
    assert.equal(
      draft.version,
      recoveryVersion,
      "same-run presentation never resets synchronization",
    );
    const sendRequest = client.sendRequest.bind(client);
    const finishCount = client.requests.filter(
      (request) => request.method === "workspace/executeCommand",
    ).length;
    client.sendNotification("textDocument/didChange", {
      textDocument: { uri: draft.effect.uri, version: draft.version },
      contentChanges: [{ text: "stale text must not replace native text" }],
    });
    await waitFor(
      () =>
        __test.messages.some(
          (notice) => notice.level === "error" && notice.text.includes("stale or duplicated"),
        ),
      "readable version error",
    );
    assert.equal(draft.document.getText(), unsent, "sync errors retain native text");
    assert.equal(
      client.requests.filter((request) => request.method === "workspace/executeCommand").length,
      finishCount,
    );
    await assert.rejects(
      sendRequest("workspace/executeCommand", {
        command: "magit.command",
        arguments: [
          {
            command: "common.confirm",
            runId: draft.runId,
            uri: draft.effect.uri,
            version: draft.version,
            source: "synchronized",
            text: "not a second text channel",
          },
        ],
      }),
    );
    await draft.document.save();
    assert.equal(
      draft.syncError,
      undefined,
      "rejected duplicate leaves accepted text/version usable",
    );
    draft.document.replaceText("explicit confirmed text\n");
    await invokeGenerated("common.confirm");
    await idle(draft);
    assert.equal(subject(), "explicit confirmed text");
    await require("../lib/drafts").open(draft.effect, draft.baseViewId);
    assert(
      !draftStore.drafts.has(draft.runId),
      "late presentation cannot resurrect a terminal run",
    );

    // Dirty close Save/Discard/Cancel is exercised by the fake editor harness.
    draft = await startDraft();
    draft.document.replaceText("saved discard checkpoint\n");
    await draft.document.save();
    draft.document.replaceText("hidden unsaved changes\n");
    const oldHead = head();
    const oldIndex = unchangedIndex();
    const oldWork = fs.readFileSync(path.join(repo, "tracked.txt"));
    __test.setQuickPickAnswer((items) => items.find((item) => item.id === "cancel"));
    await commands.get("magit.closeEdit")();
    assert(draftStore.drafts.has(draft.runId));
    assert.equal(
      draft.document.getText(),
      "hidden unsaved changes\n",
      "Cancel retains unsaved text",
    );
    assert.equal(
      progress.get(draftProgress.get(draft)),
      "begin",
      "Cancel keeps the commit pending",
    );
    assert.equal(head(), oldHead);
    __test.setQuickPickAnswer((items) => items.find((item) => item.id === "discard"));
    await commands.get("magit.closeEdit")();
    await idle(draft);
    assert(!draftStore.drafts.has(draft.runId), "Discard confirmation completed");
    assert.equal(subject(), "saved discard checkpoint");
    assert.deepEqual(unchangedIndex(), oldIndex);
    assert.deepEqual(fs.readFileSync(path.join(repo, "tracked.txt")), oldWork);

    draft = await startDraft();
    draft.document.replaceText("dirty save close\n");
    __test.setQuickPickAnswer((items) => items.find((item) => item.id === "save"));
    await commands.get("magit.closeEdit")();
    await idle(draft);
    assert.equal(subject(), "dirty save close");

    // A native close confirms disk, never the unsaved synchronized snapshot.
    // Keep real native acceptance separate: these are mocked tab API events.
    draft = await startDraft();
    draft.document.replaceText("native saved file close\n");
    await draft.document.save();
    draft.document.replaceText("unsaved text must not land\n");
    await draft.queue;
    const ownedTab = { input: { uri: draft.uri } };
    draft.tabs = new Set([ownedTab]);
    const closeHead = head();
    require("../lib/drafts").onTabsChanged({ closed: [ownedTab] });
    await draft.queue;
    assert.equal(head(), closeHead, "another matching tab still exists");
    draft.tabs = new Set([ownedTab]);
    await __test.closeDocument(draft.document);
    require("../lib/drafts").onTabsChanged({ closed: [ownedTab] });
    await draft.queue;
    await idle(draft);
    assert.equal(subject(), "native saved file close");
    const staleTab = ownedTab;
    draft = await startDraft();
    const staleHead = head();
    require("../lib/drafts").onTabsChanged({ closed: [staleTab] });
    await draft.queue;
    assert.equal(head(), staleHead, "old tab event cannot confirm a reused URI");
    await invokeGenerated("common.cancel");
    await idle(draft);

    draft = await startDraft();
    const abortText = "retain text after rejected abort\n";
    draft.document.replaceText(abortText);
    const realSendRequest = client.sendRequest.bind(client);
    const savedRequest = client.sendRequest;
    const priorHead = head();
    const priorIndex = unchangedIndex();
    const savedNotification = client.sendNotification;
    client.sendNotification = async (method, params) =>
      method === "textDocument/didChange"
        ? Promise.reject(new Error("draft synchronization rejected"))
        : savedNotification.call(client, method, params);
    const requestStart = client.requests.length;
    const noticeStart = __test.messages.length;
    await invokeGenerated("common.cancel");
    assert.equal(
      client.requests
        .slice(requestStart)
        .filter(
          (request) =>
            request.method === "workspace/executeCommand" &&
            request.params.arguments?.[0]?.command === "common.cancel",
        ).length,
      0,
      "rejected sync sends no Abort request",
    );
    assert(draftStore.drafts.has(draft.runId), "rejected sync retains registration");
    assert.equal(draft.document.getText(), abortText, "rejected sync retains bytes");
    assert(!draft.document.isClosed, "rejected sync keeps tab open");
    assert(
      __test.messages
        .slice(noticeStart)
        .some(
          (notice) =>
            notice.level === "error" && notice.text.includes("draft synchronization rejected"),
        ),
      "rejected sync reports current error",
    );
    assert.equal(head(), priorHead, "rejected sync leaves HEAD unchanged");
    assert.deepEqual(unchangedIndex(), priorIndex, "rejected sync leaves index unchanged");
    client.sendNotification = savedNotification;

    const owner = draft.client;
    draft.client = undefined;
    const unavailableNoticeStart = __test.messages.length;
    await invokeGenerated("common.cancel");
    assert.equal(draft.document.getText(), abortText, "unavailable owner retains bytes");
    assert(!draft.document.isClosed, "unavailable owner retains tab");
    assert(draftStore.drafts.has(draft.runId), "unavailable owner retains registration");
    assert(
      __test.messages
        .slice(unavailableNoticeStart)
        .some(
          (notice) => notice.level === "error" && notice.text.includes("connection is unavailable"),
        ),
      "unavailable owner reports current error",
    );
    assert.equal(head(), priorHead, "unavailable owner leaves HEAD unchanged");
    assert.deepEqual(unchangedIndex(), priorIndex, "unavailable owner leaves index unchanged");
    draft.client = owner;

    const confirmation = deferredAbort();
    let sentConfirm;
    client.sendRequest = async (method, params) => {
      if (
        method === "workspace/executeCommand" &&
        params.arguments?.[0]?.command === "common.confirm"
      ) {
        sentConfirm = params.arguments[0];
        return confirmation.response;
      }
      return realSendRequest(method, params);
    };
    const pendingConfirm = invokeGenerated("common.confirm");
    await waitFor(() => sentConfirm, "deferred confirmation response");
    let sentAbortDuringConfirm;
    client.sendRequest = async (method, params) => {
      if (
        method === "workspace/executeCommand" &&
        params.arguments?.[0]?.command === "common.cancel"
      ) {
        sentAbortDuringConfirm = params.arguments[0];
        return realSendRequest(method, params);
      }
      return realSendRequest(method, params);
    };
    await invokeGenerated("common.cancel");
    assert(sentAbortDuringConfirm, "Abort sent before confirmation release");
    confirmation.release({
      type: "confirmed",
      runId: sentConfirm.runId,
      uri: sentConfirm.uri,
      version: sentConfirm.version,
    });
    await pendingConfirm;
    assert(
      !draftStore.drafts.has(draft.runId),
      "late confirmation cannot retain invalidated draft",
    );
    assert(draft.document.isClosed, "late confirmation cannot prevent Abort teardown");
    assert.equal(head(), priorHead, "Abort during confirmation leaves HEAD unchanged");
    assert.deepEqual(
      unchangedIndex(),
      priorIndex,
      "Abort during confirmation leaves index unchanged",
    );
    client.sendRequest = savedRequest;
    await idle(draft);

    draft = await startDraft();
    draft.document.replaceText(abortText);
    await waitFor(
      () => draft.text === abortText && !draft.flushQueued,
      "retry Abort text synchronized",
    );
    for (const fake of [
      async () => {
        throw new Error("abort transport failure");
      },
      async () => ({
        type: "aborted",
        runId: "wrong",
        uri: draft.uri.toString(),
        version: draft.version,
      }),
      async () => ({
        type: "aborted",
        runId: draft.runId,
        uri: "file:///wrong",
        version: draft.version,
      }),
      async () => ({
        type: "aborted",
        runId: draft.runId,
        uri: draft.uri.toString(),
        version: draft.version - 1,
      }),
    ]) {
      client.sendRequest = async (method, params) =>
        method === "workspace/executeCommand" && params.arguments?.[0]?.command === "common.cancel"
          ? fake()
          : realSendRequest(method, params);
      await invokeGenerated("common.cancel");
      assert(draftStore.drafts.has(draft.runId), "failed Abort retains registration");
      assert.equal(draft.document.getText(), abortText, "failed Abort retains bytes");
      assert(!draft.document.isClosed, "failed Abort keeps editor open");
    }
    client.sendRequest = savedRequest;
    await invokeGenerated("common.cancel");
    assert(!draftStore.drafts.has(draft.runId), "valid Abort acknowledgement closes draft");
    assert(draft.document.isClosed, "valid Abort closes the editor");
    await idle(draft);

    draft = await startDraft();
    draft.document.replaceText("abort snapshot before later failure\n");
    await waitFor(
      () => draft.text === draft.document.getText() && !draft.flushQueued,
      "abort failure snapshot synchronized",
    );
    const failedAbort = deferredAbort();
    let sentAbort;
    client.sendRequest = async (method, params) => {
      if (
        method === "workspace/executeCommand" &&
        params.arguments?.[0]?.command === "common.cancel"
      ) {
        sentAbort = params.arguments[0];
        return failedAbort.response;
      }
      return realSendRequest(method, params);
    };
    const pendingFailedAbort = invokeGenerated("common.cancel");
    await waitFor(() => sentAbort, "deferred Abort request");
    draft.document.replaceText("retain edit after later synchronization failure\n");
    const savedLaterNotification = client.sendNotification;
    client.sendNotification = async (method, params) =>
      method === "textDocument/didChange"
        ? Promise.reject(new Error("later synchronization failure"))
        : savedLaterNotification.call(client, method, params);
    await draft.queue;
    client.sendNotification = savedLaterNotification;
    failedAbort.release({
      type: "aborted",
      runId: sentAbort.runId,
      uri: sentAbort.uri,
      version: sentAbort.version,
    });
    await pendingFailedAbort;
    assert(draftStore.drafts.has(draft.runId), "late Abort after failure retains registration");
    assert.equal(
      draft.document.getText(),
      "retain edit after later synchronization failure\n",
      "late Abort after failure retains newer bytes",
    );
    assert(!draft.document.isClosed, "late Abort after failure keeps editor open");

    await draft.document.save();
    const editAbort = deferredAbort();
    sentAbort = undefined;
    client.sendRequest = async (method, params) => {
      if (
        method === "workspace/executeCommand" &&
        params.arguments?.[0]?.command === "common.cancel"
      ) {
        sentAbort = params.arguments[0];
        return editAbort.response;
      }
      return realSendRequest(method, params);
    };
    const pendingEditAbort = invokeGenerated("common.cancel");
    await waitFor(() => sentAbort, "deferred Abort before native edit");
    const sentVersion = sentAbort.version;
    draft.document.replaceText("retain native edit made during Abort\n");
    await waitFor(
      () => draft.version > sentVersion && draft.text === draft.document.getText(),
      "native edit synchronized during Abort",
    );
    editAbort.release({
      type: "aborted",
      runId: sentAbort.runId,
      uri: sentAbort.uri,
      version: sentVersion,
    });
    await pendingEditAbort;
    assert(draftStore.drafts.has(draft.runId), "Abort snapshot cannot close over newer edit");
    assert.equal(draft.document.getText(), "retain native edit made during Abort\n");
    assert(
      __test.messages.some(
        (notice) => notice.level === "error" && notice.text.includes("changed while aborting"),
      ),
    );

    const teardownText = "retain native edit during acknowledged Abort teardown\n";
    const teardownContext = deferredAbort();
    const savedExecute = vscodeCommands.executeCommand;
    let waitingOnTeardownContext = false;
    const teardownCommands = [];
    client.sendRequest = async (method, params) => {
      if (
        method === "workspace/executeCommand" &&
        params.arguments?.[0]?.command === "common.cancel"
      ) {
        const sent = params.arguments[0];
        return { type: "aborted", runId: sent.runId, uri: sent.uri, version: sent.version };
      }
      return realSendRequest(method, params);
    };
    vscodeCommands.executeCommand = async (name, ...args) => {
      teardownCommands.push(name);
      if (
        name === "setContext" &&
        args[0] === "magit.editing" &&
        args[1] === false &&
        !waitingOnTeardownContext
      ) {
        waitingOnTeardownContext = true;
        return teardownContext.response;
      }
      return savedExecute(name, ...args);
    };
    const pendingTeardownAbort = invokeGenerated("common.cancel");
    await waitFor(() => waitingOnTeardownContext, "acknowledged Abort teardown context");
    draft.document.replaceText(teardownText);
    teardownContext.release();
    await pendingTeardownAbort;
    vscodeCommands.executeCommand = savedExecute;
    assert.equal(draft.document.getText(), teardownText, "teardown edit retains exact bytes");
    assert(draftStore.drafts.has(draft.runId), "teardown edit retains registration");
    assert(!draft.document.isClosed, "teardown edit keeps tab open");
    assert.equal(
      __test.contextValues.get("magit.editing"),
      true,
      "teardown edit restores registered context",
    );
    assert(
      !teardownCommands.includes("workbench.action.files.revert"),
      "teardown edit prevents revert",
    );
    assert(
      teardownCommands.includes("workbench.action.files.resetActiveEditorReadonlyInSession"),
      "teardown edit restores editable state",
    );

    client.sendRequest = savedRequest;
    let healthyReverts = 0;
    vscodeCommands.executeCommand = async (name, ...args) => {
      if (name === "workbench.action.files.revert") healthyReverts += 1;
      return savedExecute(name, ...args);
    };
    await invokeGenerated("common.cancel");
    vscodeCommands.executeCommand = savedExecute;
    assert(
      !draftStore.drafts.has(draft.runId),
      "healthy exact Abort closes after retained edit retry",
    );
    assert(draft.document.isClosed, "healthy exact Abort closes editor once");
    assert.equal(healthyReverts, 1, "healthy exact Abort reverts once");
    await idle(draft);

    draft = await startDraft();
    const disconnectText = "retain text after disconnect during Abort teardown\n";
    draft.document.replaceText(disconnectText);
    await waitFor(
      () => draft.text === disconnectText && !draft.flushQueued,
      "disconnect teardown text synchronized",
    );
    const teardownGate = deferredAbort();
    let waitingOnTeardownGate = false;
    const disconnectTeardownCommands = [];
    client.sendRequest = async (method, params) => {
      if (
        method === "workspace/executeCommand" &&
        params.arguments?.[0]?.command === "common.cancel"
      ) {
        const sent = params.arguments[0];
        return { type: "aborted", runId: sent.runId, uri: sent.uri, version: sent.version };
      }
      return realSendRequest(method, params);
    };
    vscodeCommands.executeCommand = async (name, ...args) => {
      disconnectTeardownCommands.push(name);
      if (
        name === "workbench.action.files.setActiveEditorReadonlyInSession" &&
        !waitingOnTeardownGate
      ) {
        waitingOnTeardownGate = true;
        return teardownGate.response;
      }
      return savedExecute(name, ...args);
    };
    const pendingDisconnectAbort = invokeGenerated("common.cancel");
    await waitFor(() => waitingOnTeardownGate, "acknowledged Abort teardown gate");
    require("../lib/drafts").disconnect("disconnect during Abort teardown");
    teardownGate.release();
    await pendingDisconnectAbort;
    vscodeCommands.executeCommand = savedExecute;
    assert.equal(
      draft.document.getText(),
      disconnectText,
      "disconnect teardown retains exact bytes",
    );
    assert(draftStore.drafts.has(draft.runId), "disconnect teardown retains registration");
    assert(!draft.document.isClosed, "disconnect teardown keeps tab open");
    assert(
      !disconnectTeardownCommands.includes("workbench.action.files.revert"),
      "disconnect teardown prevents revert",
    );
    assert(
      disconnectTeardownCommands.includes(
        "workbench.action.files.resetActiveEditorReadonlyInSession",
      ),
      "disconnect teardown restores editable state",
    );
    draft.disconnected = false;
    client.sendRequest = savedRequest;
    await invokeGenerated("common.cancel");
    assert(
      !draftStore.drafts.has(draft.runId),
      "healthy exact Abort closes after disconnect retry",
    );
    await idle(draft);

    for (const action of ["abort", "progress-cancel"]) {
      draft = await startDraft();
      const priorHead = head();
      const priorIndex = unchangedIndex();
      if (action === "abort") await invokeGenerated("common.cancel");
      if (action === "progress-cancel") {
        const token = draftProgress.get(draft);
        assert.equal(progress.get(token), "begin", "cancel the current active commit");
        client.sendNotification("window/workDoneProgress/cancel", { token });
      }
      await idle(draft);
      assert.equal(head(), priorHead, action);
      assert.deepEqual(unchangedIndex(), priorIndex, action);
      if (draftStore.drafts.has(draft.runId)) await commands.get("magit.cancelEdit")();
    }
    const synced = client.notes.filter((note) => note.method === "textDocument/didChange");
    assert(
      synced.every(
        (note) =>
          note.params.contentChanges.length === 1 &&
          Object.keys(note.params.contentChanges[0]).join() === "text",
      ),
    );

    // The displayed text is always the server's last render; never recomposed.
    assert.match(statusEditor.document.getText(), /Unstaged changes/);
    assert.equal(__test.output.filter((line) => line.startsWith("unknown effect")).length, 0);

    // Quit is dispatched through the registered command and scoped to the
    // repository that owns the originating view. This harness uses mocked
    // VSCode APIs; it is not evidence of native tab behavior.
    const { store } = require("../lib/store");
    const secondRepo = createRepository();
    __test.setWorkspaceFolder(secondRepo);
    await commands.get("magit.open")();
    await waitFor(() => store.views.size === 2, "second repository view");
    const secondView = () =>
      Array.from(store.views.values()).find((state) => state.repo === secondRepo);
    await waitFor(
      () =>
        secondView() &&
        secondView().id === store.activeViewId &&
        __test.activeEditor &&
        __test.activeEditor.document.uri.toString() === secondView().uri.toString() &&
        /Unstaged changes/.test(__test.activeEditor.document.getText()),
      "second repository status view ready",
    );
    await commands.get("magit.invoke")({ command: "common.quit" });
    assert.equal(store.views.size, 1, "quit removes only the originating repository view");
    assert.equal(Array.from(store.views.values())[0].repo, repo);
    __test.setWorkspaceFolder(repo);
    await commands.get("magit.open")();
    await commands.get("magit.invoke")({ command: "common.quit" });
    assert.equal(store.views.size, 0, "root quit removes the affected stored view");
    await commands.get("magit.open")();
    await waitFor(() => store.views.size === 1, "reopened repository view");
    fs.rmSync(secondRepo, { recursive: true, force: true });
    await require("./selection-integration").checkGitSelections({
      vscode: vscodeModule,
      commands,
      client,
    });
    // A disconnected owner cannot finish or restart against a new connection;
    // server shutdown aborts the pending helper, while native text is retained.
    __test.setWorkspaceFolder(repo);
    await commands.get("magit.open")();
    fs.writeFileSync(path.join(repo, "tracked.txt"), "disconnect draft probe\n");
    git(repo, ["add", "tracked.txt"]);
    await commands.get("magit.invoke")({ command: "commit.create" });
    await waitFor(
      () => [...draftStore.drafts.values()].some((record) => record.attached),
      "disconnect draft",
    );
    const disconnected = [...draftStore.drafts.values()].find((record) => record.attached);
    disconnected.document.replaceText("retain disconnected text\n");
    const disconnectHead = head();
    await extension.deactivate();
    assert.equal(head(), disconnectHead, "disconnect cancels the pending commit");
    assert.equal(disconnected.document.getText(), "retain disconnected text\n");
    assert(disconnected.disconnected);
  } finally {
    await extension.deactivate();
  }

  const exited = await Promise.race([
    instances.at(-1).exited.then(() => true),
    delay(5000).then(() => false),
  ]);
  assert(exited, "the language server exited after deactivate");
  fs.rmSync(repo, { recursive: true, force: true });
}

run()
  .then(() => {
    console.log("ok node integration");
  })
  .catch((error) => {
    console.error(error && error.stack ? error.stack : error);
    for (const client of instances) {
      if (client.stderr) console.error(`--- magit-lsp stderr ---\n${client.stderr}`);
    }
    for (const message of __test.messages) {
      console.error(`--- ${message.level}: ${message.text}`);
    }
    for (const line of __test.output) {
      console.error(`--- output: ${line}`);
    }
    process.exitCode = 1;
  });
