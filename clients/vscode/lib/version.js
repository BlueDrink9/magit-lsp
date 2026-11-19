'use strict';

const fs = require('node:fs');
const path = require('node:path');

/** Rich protocol session version this client declares and requires. */
const PROTOCOL_VERSION = 1;

/** Resolve a diagnostic command through PATH without changing the launch command. */
function resolveCommand(command, env = process.env, platform = process.platform) {
  if (command.includes('/') || command.includes('\\')) return command;
  const windows = platform === 'win32';
  const delimiter = windows ? ';' : path.delimiter;
  const pathext = (env.PATHEXT || '.COM;.EXE;.BAT;.CMD').split(';');
  const hasSuffix = windows && pathext.some(ext => command.toLowerCase().endsWith(ext.toLowerCase()));
  const extensions = windows ? (hasSuffix ? [''] : pathext) : [''];
  for (const directory of (env.PATH || '').split(delimiter)) {
    for (const extension of extensions) {
      let candidate = path.resolve(directory || '.', command + extension);
      if (windows) {
        try {
          const match = fs.readdirSync(path.dirname(candidate)).find(name => name.toLowerCase() === path.basename(candidate).toLowerCase());
          if (match) candidate = path.join(path.dirname(candidate), match);
        } catch {}
      }
      try {
        const stat = fs.statSync(candidate);
        if (stat.isFile()) {
          if (windows) return candidate;
          fs.accessSync(candidate, fs.constants.X_OK);
          return candidate;
        }
      } catch {}
    }
  }
  return command;
}

/** Describe a server version mismatch or missing server metadata. */
function warning(extensionVersion, serverVersion, executable) {
  if (serverVersion === extensionVersion) return undefined;
  return `magit-lsp version mismatch: extension ${extensionVersion}, server ${serverVersion || 'unknown'} (${executable})`;
}

/**
 * Report an incompatible rich protocol advertisement, if any.
 *
 * The advertisement is the version of every accepted rich session, so it must
 * be a positive integer equal to the session version this client declared. A
 * missing, malformed or different value is a compatibility failure: there is
 * no fallback, default or retry.
 *
 * @param {any} result initialize result
 * @param {number} required the rich protocol session version this client declared
 * @returns {string|undefined} readable mismatch naming required and advertised versions
 */
function protocolError(result, required) {
  const advertised = result?.capabilities?.experimental?.magit?.protocolVersion;
  if (Number.isInteger(advertised) && advertised > 0 && advertised === required) {
    return undefined;
  }
  let shown = 'none';
  if (advertised !== undefined) {
    shown = Number.isInteger(advertised)
      ? String(advertised)
      : JSON.stringify(advertised) ?? String(advertised);
  }
  return `rich protocol mismatch: required version ${required}, server advertised ${shown}`;
}

exports.PROTOCOL_VERSION = PROTOCOL_VERSION;
exports.protocolError = protocolError;
exports.resolveCommand = resolveCommand;
exports.warning = warning;
