'use strict';

const fs = require('node:fs');
const { spawnSync } = require('node:child_process');

const { releaseVersion, resolveServer } = require('./server-download');

/** Values accepted for `magit-lsp.server.source`. */
const SOURCES = Object.freeze(['auto', 'system', 'managed']);

/** Bare command probed on PATH for a user-installed server. */
const PATH_COMMAND = 'magit-lsp';

/** Bound on the version probe so a hung executable cannot block startup. */
const PROBE_MILLISECONDS = 10000;

/** Exact `--version` output of a magit-lsp server. */
const VERSION_OUTPUT = /^magit-lsp (\d+\.\d+\.\d+)$/;

/** A readable, actionable failure while choosing the server executable. */
class ServerSourceError extends Error {
  /** @param {string} reason user-facing reason without the product prefix */
  constructor(reason) {
    super(`magit-lsp: ${reason}`);
    this.name = 'ServerSourceError';
  }
}

/**
 * Throw a readable server-source failure.
 *
 * @param {string} reason
 * @returns {never}
 */
function fail(reason) {
  throw new ServerSourceError(reason);
}

/**
 * Normalize the configured source, defaulting an absent value to `auto`.
 *
 * @param {unknown} value `magit-lsp.server.source` setting
 * @returns {'auto'|'system'|'managed'}
 */
function sourceSetting(value) {
  if (value === undefined || value === null || value === '') return 'auto';
  if (typeof value !== 'string' || !SOURCES.includes(value)) {
    fail(
      `unsupported "magit-lsp.server.source" value ${JSON.stringify(value)}; ` +
        'expected "auto", "system" or "managed"',
    );
  }
  return /** @type {'auto'|'system'|'managed'} */ (value);
}

/**
 * Reject an explicit server path that is not an existing executable file.
 *
 * The user's path is authoritative, so an unusable one is a hard error rather
 * than a reason to fall through to PATH or a download.
 *
 * @param {string} configured `magit-lsp.server.path` setting
 * @returns {void}
 */
function validateExplicit(configured) {
  const advice = 'fix "magit-lsp.server.path" or set "magit-lsp.server.source" to "managed"';
  let stat;
  try {
    stat = fs.statSync(configured);
  } catch {
    fail(`configured server path "${configured}" does not exist; ${advice}`);
  }
  if (!stat.isFile()) {
    fail(`configured server path "${configured}" is not a file; ${advice}`);
  }
  try {
    fs.accessSync(configured, fs.constants.X_OK);
  } catch {
    fail(`configured server path "${configured}" is not executable; ${advice}`);
  }
}

/**
 * Probe the bare `magit-lsp` command on PATH for its identity and version.
 *
 * A server must exit zero and print exactly `magit-lsp <X.Y.Z>`; anything else
 * is reported as a probe failure instead of being launched.
 *
 * @param {NodeJS.ProcessEnv} env environment supplying PATH
 * @returns {{found: boolean, version?: string, command?: string, error?: string}}
 */
function probePath(env) {
  const result = spawnSync(PATH_COMMAND, ['--version'], {
    env,
    encoding: 'utf8',
    timeout: PROBE_MILLISECONDS,
    windowsHide: true,
  });
  if (result.error && /** @type {NodeJS.ErrnoException} */ (result.error).code === 'ENOENT') {
    return { found: false };
  }
  if (result.error) return { found: true, error: result.error.message || String(result.error) };
  if (result.status !== 0) {
    return { found: true, error: `--version exited ${result.status ?? result.signal}` };
  }
  const match = VERSION_OUTPUT.exec(String(result.stdout ?? '').trim());
  if (!match) {
    return { found: true, error: '--version did not print "magit-lsp <X.Y.Z>"' };
  }
  return { found: true, version: match[1], command: PATH_COMMAND };
}

/**
 * @typedef {object} ServerSourceOptions
 * @property {unknown} [source] `magit-lsp.server.source` setting; defaults to auto
 * @property {unknown} [path] `magit-lsp.server.path` setting; defaults to empty
 * @property {string} version pinned extension version
 * @property {string} cacheRoot extension global storage for managed installs
 * @property {NodeJS.ProcessEnv} [env] environment used for the PATH probe
 * @property {typeof resolveServer} [managed] managed resolver; tests inject a fixture
 * @property {{platform?: string, arch?: string, glibc?: string|null}} [runtime] host facts for managed installs
 */

/**
 * @typedef {object} ResolvedServerSource
 * @property {string} command executable or PATH name the client starts
 * @property {'explicit'|'system'|'managed'} origin where the command came from
 * @property {string|null} version reported version, or null for an explicit path
 */

/**
 * Choose the server executable from the explicit path, PATH or managed release.
 *
 * `auto` prefers an explicit path, then a PATH binary whose version equals the
 * pinned extension version, then the managed pinned download. `system` never
 * downloads and reports a readable failure when PATH is missing or
 * incompatible. `managed` ignores the explicit path and always resolves the
 * pinned release. An explicit path is validated but never version-gated: it is
 * the user's override and the post-connect mismatch warning still applies.
 *
 * @param {ServerSourceOptions} options
 * @returns {Promise<ResolvedServerSource>}
 */
async function resolveServerSource(options) {
  const source = sourceSetting(options?.source);
  const pinned = releaseVersion(options?.version);
  const configured = typeof options?.path === 'string' ? options.path.trim() : '';
  const resolveManaged = options?.managed ?? resolveServer;

  /**
   * Resolve the pinned release through the existing downloader.
   *
   * @returns {Promise<ResolvedServerSource>}
   */
  async function managed() {
    const resolved = await resolveManaged({
      version: pinned,
      cacheRoot: options?.cacheRoot,
      ...(options?.runtime ? { runtime: options.runtime } : {}),
    });
    return { command: resolved.executable, origin: 'managed', version: resolved.version };
  }

  if (source === 'managed') return managed();

  if (configured) {
    validateExplicit(configured);
    return { command: configured, origin: 'explicit', version: null };
  }

  const probe = probePath(options?.env ?? process.env);
  if (source === 'system') {
    const advice =
      'set "magit-lsp.server.path" to a magit-lsp executable or set ' +
      '"magit-lsp.server.source" to "managed"';
    if (!probe.found) {
      fail(`"magit-lsp.server.source" is "system" but no magit-lsp was found on PATH; ${advice}`);
    }
    if (probe.error) {
      fail(`the magit-lsp found on PATH cannot report its version (${probe.error}); ${advice}`);
    }
    if (probe.version !== pinned) {
      fail(
        `the magit-lsp found on PATH reports version ${probe.version} but this extension ` +
          `requires ${pinned}; ${advice}`,
      );
    }
    /** @type {ResolvedServerSource} */
    const resolved = {
      command: /** @type {string} */ (probe.command),
      origin: 'system',
      version: /** @type {string} */ (probe.version),
    };
    return resolved;
  }

  if (probe.found && probe.version === pinned) {
    /** @type {ResolvedServerSource} */
    const resolved = {
      command: /** @type {string} */ (probe.command),
      origin: 'system',
      version: /** @type {string} */ (probe.version),
    };
    return resolved;
  }
  return managed();
}

exports.resolveServerSource = resolveServerSource;
