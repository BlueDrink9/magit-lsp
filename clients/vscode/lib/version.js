'use strict';

const fs = require('node:fs');
const path = require('node:path');

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

exports.resolveCommand = resolveCommand;
exports.warning = warning;
