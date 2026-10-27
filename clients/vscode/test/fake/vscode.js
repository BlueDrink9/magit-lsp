'use strict';

/**
 * The scripted `vscode` module used by the node integration test.
 *
 * It implements only the API surface `lib/` and `extension.js` call, records
 * every widget interaction for assertions and lets the test script answers to
 * QuickPick and input prompts. Nothing here parses server text or structure.
 */

const fs = require('node:fs');

/** All observable fake-window state, reset between test cases. */
const state = {
  commands: new Map(),
  providers: new Map(),
  documents: new Map(),
  editors: [],
  activeEditor: undefined,
  messages: [],
  output: [],
  contextValues: new Map(),
  executed: [],
  quickPicks: [],
  lastQuickPick: undefined,
  quickPickAnswer: undefined,
  inputAnswers: [],
  informationAnswer: undefined,
  closeHandlers: [],
  activeEditorHandlers: [],
  workspaceFolder: undefined,
  configuration: {},
  clipboard: [],
  semanticProviders: [],
};

/** A VSCode-like URI with `file`, `magit` and provider documents. */
class Uri {
  /**
   * @param {{scheme: string, authority?: string, path?: string}} parts
   */
  constructor({ scheme, authority = '', path = '' }) {
    Object.assign(this, { scheme, authority, path });
  }

  /** @param {object} parts */
  static from(parts) {
    return new Uri(parts);
  }

  /** @param {string} value */
  static parse(value) {
    const match = /^([^:]+):\/\/([^/]*)(\/.*)$/.exec(value);
    if (!match) {
      return new Uri({ scheme: 'file', path: value });
    }
    return new Uri({ scheme: match[1], authority: match[2], path: match[3] });
  }

  /** @param {string} file */
  static file(file) {
    return new Uri({ scheme: 'file', path: file });
  }

  /** The local filesystem path of a `file` URI. */
  get fsPath() {
    return this.path;
  }

  toString() {
    return `${this.scheme}://${this.authority}${this.path}`;
  }
}

/** A VSCode-compatible event emitter. */
class EventEmitter {
  constructor() {
    this.listeners = [];
    this.event = (listener) => {
      this.listeners.push(listener);
      return { dispose: () => {} };
    };
  }

  /** @param {unknown} value */
  fire(value) {
    for (const listener of this.listeners) listener(value);
  }

  dispose() {
    this.listeners = [];
  }
}

/** A zero-based text position. */
class Position {
  /** @param {number} line @param {number} character */
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}

/** A selection with an anchor and an active end. */
class Selection {
  /** @param {Position} anchor @param {Position} active */
  constructor(anchor, active) {
    this.anchor = anchor;
    this.active = active;
    const ordered = (left, right) => left.line < right.line
      || (left.line === right.line && left.character <= right.character);
    this.start = ordered(anchor, active) ? anchor : active;
    this.end = ordered(anchor, active) ? active : anchor;
    this.isEmpty = anchor.line === active.line && anchor.character === active.character;
  }
}

/** A range between two positions. */
class Range {
  /** @param {Position} start @param {Position} end */
  constructor(start, end) {
    this.start = start;
    this.end = end;
  }
}

/** The semantic-token legend the server declares. */
class SemanticTokensLegend {
  /** @param {string[]} tokenTypes @param {string[]} tokenModifiers */
  constructor(tokenTypes, tokenModifiers) {
    this.tokenTypes = tokenTypes;
    this.tokenModifiers = tokenModifiers;
  }
}

/** Decoded semantic tokens. */
class SemanticTokens {
  /** @param {Uint32Array} data @param {string} [resultId] */
  constructor(data, resultId) {
    this.data = data;
    this.resultId = resultId;
  }
}

/**
 * Read the text a provider document currently exposes.
 *
 * @param {Uri} uri
 * @returns {string}
 */
function providerText(uri) {
  const provider = state.providers.get(uri.scheme);
  return provider ? provider.provideTextDocumentContent(uri) : '';
}

/**
 * A text document over a provider URI, a real file or in-memory content.
 *
 * @param {Uri} uri
 * @param {string} content
 * @returns {object}
 */
function makeDocument(uri, content) {
  const lines = () => content.split('\n');
  return {
    uri,
    languageId: 'plaintext',
    getText: () => content,
    get lineCount() {
      return lines().length;
    },
    lineAt(line) {
      return { text: lines()[line] || '' };
    },
    async save() {
      return true;
    },
  };
}

/**
 * Open or fetch the fake document for a URI.
 *
 * Provider documents are re-read from their provider on every access so a
 * `view/render` is visible exactly as a real virtual document reflects it.
 *
 * @param {Uri} uri
 * @param {string} [content]
 * @returns {object}
 */
function openDocument(uri, content) {
  const key = uri.toString();
  if (state.providers.has(uri.scheme)) {
    const text = providerText(uri);
    const document = makeDocument(uri, text);
    Object.defineProperty(document, 'getText', { value: () => providerText(uri) });
    Object.defineProperty(document, 'lineCount', {
      get: () => providerText(uri).split('\n').length,
    });
    Object.defineProperty(document, 'lineAt', {
      value: (line) => ({ text: providerText(uri).split('\n')[line] || '' }),
    });
    state.documents.set(key, document);
    return document;
  }
  const body = content !== undefined ? content : (fs.existsSync(uri.path) ? fs.readFileSync(uri.path, 'utf8') : '');
  const document = makeDocument(uri, body);
  state.documents.set(key, document);
  return document;
}

/**
 * The editor currently showing a document, creating one when needed.
 *
 * @param {object} document
 * @returns {object}
 */
function editorFor(document) {
  let editor = state.editors.find(
    (candidate) => candidate.document.uri.toString() === document.uri.toString()
  );
  if (!editor) {
    const position = new Position(0, 0);
    editor = {
      document,
      selection: new Selection(position, position),
      revealRange() {},
    };
    state.editors.push(editor);
  }
  return editor;
}

/**
 * Create a scriptable QuickPick.
 *
 * @returns {object}
 */
function createQuickPick() {
  const quickPick = {
    title: '',
    placeholder: '',
    items: [],
    selectedItems: [],
    value: '',
    visible: false,
    acceptHandler: undefined,
    changeHandler: undefined,
    hideHandler: undefined,
    show() {
      this.visible = true;
    },
    hide() {
      this.visible = false;
      if (this.hideHandler) this.hideHandler();
    },
    dispose() {
      this.visible = false;
    },
    onDidAccept(handler) {
      this.acceptHandler = handler;
    },
    onDidChangeValue(handler) {
      this.changeHandler = handler;
    },
    onDidHide(handler) {
      this.hideHandler = handler;
    },
  };
  Object.defineProperty(quickPick, 'value', {
    get() {
      return this._value || '';
    },
    set(next) {
      this._value = next;
      if (this.changeHandler) this.changeHandler(next);
    },
  });
  /** Select one item as a user would and fire accept. @param {object} item */
  quickPick.select = function select(item) {
    this.selectedItems = [item];
    if (this.acceptHandler) this.acceptHandler();
  };
  state.quickPicks.push(quickPick);
  state.lastQuickPick = quickPick;
  return quickPick;
}

const vscode = {
  Uri,
  EventEmitter,
  Position,
  Selection,
  Range,
  SemanticTokensLegend,
  SemanticTokens,
  QuickPickItemKind: { Separator: -1, Default: 0 },
  TextEditorRevealType: { InCenterIfOutsideViewport: 0 },

  workspace: {
    get workspaceFolders() {
      return state.workspaceFolder
        ? [{ uri: Uri.file(state.workspaceFolder) }]
        : undefined;
    },
    getConfiguration(section) {
      return {
        /**
         * @param {string} name
         * @param {unknown} [fallback]
         */
        get(name, fallback) {
          const scoped = state.configuration[`${section}.${name}`];
          return scoped === undefined ? fallback : scoped;
        },
      };
    },
    registerTextDocumentContentProvider(scheme, provider) {
      state.providers.set(scheme, provider);
      return { dispose() {} };
    },
    onDidCloseTextDocument(handler) {
      state.closeHandlers.push(handler);
      return { dispose() {} };
    },
    async openTextDocument(input) {
      if (input instanceof Uri) {
        return openDocument(input);
      }
      return openDocument(Uri.from({ scheme: 'untitled', path: '/untitled' }), input.content || '');
    },
  },

  languages: {
    async setTextDocumentLanguage(document, languageId) {
      document.languageId = languageId;
      // Real VSCode closes and reopens the document for a language change.
      for (const handler of state.closeHandlers) await handler(document);
      return document;
    },
    registerDocumentSemanticTokensProvider(selector, provider, legend) {
      state.semanticProviders.push({ selector, provider, legend });
      return { dispose() {} };
    },
  },

  commands: {
    registerCommand(name, callback) {
      state.commands.set(name, callback);
      return { dispose() {} };
    },
    async executeCommand(name, ...args) {
      state.executed.push({ name, args });
      if (name === 'setContext') {
        state.contextValues.set(args[0], args[1]);
      }
    },
  },

  window: {
    get activeTextEditor() {
      return state.activeEditor;
    },
    get visibleTextEditors() {
      return state.editors;
    },
    tabGroups: {
      get all() {
        return state.editors.map((editor) => ({
          tabs: [{ input: { uri: editor.document.uri } }],
        }));
      },
      async close() {
        return true;
      },
    },
    createOutputChannel() {
      return {
        /** @param {string} line */
        appendLine(line) {
          state.output.push(line);
        },
        dispose() {},
      };
    },
    onDidChangeActiveTextEditor(handler) {
      state.activeEditorHandlers.push(handler);
      return { dispose() {} };
    },
    async showTextDocument(document) {
      const editor = editorFor(document);
      state.activeEditor = editor;
      for (const handler of state.activeEditorHandlers) handler(editor);
      return editor;
    },
    createQuickPick,
    async showQuickPick(items) {
      if (state.quickPickAnswer) {
        return state.quickPickAnswer(items);
      }
      return items && items[0];
    },
    async showInputBox() {
      return state.inputAnswers.shift();
    },
    showErrorMessage(text) {
      state.messages.push({ level: 'error', text });
    },
    showWarningMessage(text) {
      state.messages.push({ level: 'warn', text });
    },
    showInformationMessage(text) {
      state.messages.push({ level: 'info', text });
      if (state.informationAnswer) return state.informationAnswer;
      return undefined;
    },
  },

  env: {
    clipboard: {
      /** @param {string} text */
      async writeText(text) {
        state.clipboard.push(text);
      },
    },
  },

  /** Test-only controls; the extension never calls these. */
  __test: {
    /** Drop every recorded interaction. */
    reset() {
      state.commands.clear();
      state.providers.clear();
      state.documents.clear();
      state.editors.length = 0;
      state.activeEditor = undefined;
      state.messages.length = 0;
      state.output.length = 0;
      state.contextValues.clear();
      state.executed.length = 0;
      state.quickPicks.length = 0;
      state.lastQuickPick = undefined;
      state.quickPickAnswer = undefined;
      state.inputAnswers.length = 0;
      state.informationAnswer = undefined;
      state.closeHandlers.length = 0;
      state.activeEditorHandlers.length = 0;
      state.workspaceFolder = undefined;
      state.configuration = {};
      state.clipboard.length = 0;
      state.semanticProviders.length = 0;
    },
    get commands() {
      return state.commands;
    },
    get documents() {
      return state.documents;
    },
    get editors() {
      return state.editors;
    },
    get messages() {
      return state.messages;
    },
    get output() {
      return state.output;
    },
    get contextValues() {
      return state.contextValues;
    },
    get quickPicks() {
      return state.quickPicks;
    },
    get lastQuickPick() {
      return state.lastQuickPick;
    },
    get clipboard() {
      return state.clipboard;
    },
    get activeEditor() {
      return state.activeEditor;
    },
    /** @param {string} folder */
    setWorkspaceFolder(folder) {
      state.workspaceFolder = folder;
    },
    /** @param {Record<string, unknown>} values dotted `section.key` settings */
    setConfiguration(values) {
      Object.assign(state.configuration, values);
    },
    /** @param {string|undefined} answer */
    queueInputAnswer(answer) {
      state.inputAnswers.push(answer);
    },
    /** @param {(items: object[]) => unknown} answer */
    setQuickPickAnswer(answer) {
      state.quickPickAnswer = answer;
    },
    /** @param {unknown} answer */
    setInformationAnswer(answer) {
      state.informationAnswer = answer;
    },
    /** Simulate the user closing a document. @param {object} document */
    async closeDocument(document) {
      for (const handler of state.closeHandlers) await handler(document);
    },
  },
};

module.exports = vscode;
