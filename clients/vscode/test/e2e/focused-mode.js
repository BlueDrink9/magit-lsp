'use strict';

const modes = [
  ['MAGIT_VSCODE_VERSION_ONLY', 'version'],
  ['MAGIT_VSCODE_VERSION_WARNING_ONLY', 'version'],
  ['MAGIT_VSCODE_EDIT_ONLY', 'edit'],
  ['MAGIT_VSCODE_HEAD_ONLY', 'head'],
  ['MAGIT_VSCODE_DOWNLOAD_ONLY', 'download'],
  ['MAGIT_VSCODE_VSIX_INSTALL_ONLY', 'vsix-install'],
  ['MAGIT_VSCODE_SERVER_COMMANDS_ONLY', 'server-commands'],
  ['MAGIT_VSCODE_A34_ONLY', 'a34'],
  ['MAGIT_VSCODE_A33_ONLY', 'a33'],
  ['MAGIT_VSCODE_A29_ONLY', 'a29'],
  ['MAGIT_VSCODE_A06_ONLY', 'a06'],
];

/** Select the one suite branch requested by launcher environment. */
function selectSuiteMode(env) {
  if (env.MAGIT_VSCODE_SUITE_MODE) return env.MAGIT_VSCODE_SUITE_MODE;
  return modes.find(([key]) => env[key] === '1')?.[1] || 'generic';
}

/** Return the explicit suite branch environment passed across the launcher boundary. */
function withSuiteMode(env) {
  return { MAGIT_VSCODE_SUITE_MODE: selectSuiteMode(env) };
}

module.exports = { selectSuiteMode, withSuiteMode };
