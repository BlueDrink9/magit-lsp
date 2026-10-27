'use strict';

/**
 * Mutable state shared by the extension's modules.
 *
 * The extension is one session per window: the language client, the open
 * server-owned views and the Git edit documents all belong together. Keeping
 * that state in one object lets the view, menu, edit and command modules stay
 * small without reintroducing module singletons per feature.
 */

/** Virtual-document scheme for every server-owned view. */
const SCHEME = 'magit';

const store = {
  /** Semantic-token invalidation event, registered after initialization. */
  semanticChanged: undefined,

  /** @type {import('vscode-languageclient/node').LanguageClient|undefined} */
  client: undefined,

  /** @type {Promise<void>|undefined} */
  clientReady: undefined,

  /** @type {import('vscode').OutputChannel|undefined} */
  output: undefined,

  /** @type {import('vscode').EventEmitter<import('vscode').Uri>|undefined} */
  documentsChanged: undefined,

  /** @type {string|undefined} */
  activeViewId: undefined,

  /** @type {string|undefined} */
  quickPickViewId: undefined,

  /** @type {string|undefined} */
  activeDraftId: undefined,

  /** All views the server opened in this window, keyed by view id. */
  views: new Map(),

  /** Virtual document URI -> view id. */
  uriToView: new Map(),

  /** Renders that arrived before their descriptor's effect. */
  pendingRenders: new Map(),

  /** Repository path -> in-flight status open, including its first render. */
  statusOpens: new Map(),

  /** Managed draft documents, keyed by the opaque server run id. */
  drafts: new Map(),

  /** Managed URI -> run id; document teardown is detach, never confirmation. */
  draftUris: new Map(),

  /** Shutdown blocks draft barriers before native editor teardown begins. */
  disposing: false,
};

module.exports = { SCHEME, store };
