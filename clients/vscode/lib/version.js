'use strict';

const fs = require('node:fs');
const path = require('node:path');

/** Resolve a command through PATH without changing the command used to launch it. */
function resolveCommand(command, env = process.env) {
  if (command.includes('/') || command.includes('\\')) return command;
  const extensions = process.platform === 'win32'
    ? (env.PATHEXT || '.COM;.EXE;.BAT;.CMD').split(';') : [''];
  for (const directory of (env.PATH || '').split(path.delimiter)) {
    for (const extension of extensions) {
      const candidate = path.join(directory || '.', command + extension);
      try { if (fs.statSync(candidate).isFile()) return candidate; } catch {}
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
