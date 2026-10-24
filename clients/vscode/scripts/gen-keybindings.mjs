#!/usr/bin/env node

// Regenerate `contributes.keybindings` from the server's static manifest.
//
// Usage: node clients/vscode/scripts/gen-keybindings.mjs [--check]
//
// `--check` compares instead of writing, for CI and for verifying that the
// committed manifest is current.

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const clientRoot = resolve(here, '..');
const repoRoot = resolve(clientRoot, '..', '..');
const manifestPath = join(clientRoot, 'package.json');
const checkOnly = process.argv.includes('--check');

/** Build current sources; only an explicit override may select another binary. */
function serverCommand() {
  if (process.env.MAGIT_LSP_BIN) {
    return { command: process.env.MAGIT_LSP_BIN, args: [] };
  }
  return {
    command: 'cargo',
    args: ['run', '--quiet', '--manifest-path', join(repoRoot, 'Cargo.toml'), '--'],
  };
}

const server = serverCommand();
const output = execFileSync(
  server.command,
  [...server.args, 'keymap', '--format', 'vscode', '--profile', 'standard'],
  { cwd: repoRoot, encoding: 'utf8' }
);

const keybindings = JSON.parse(output);
const pkg = JSON.parse(readFileSync(manifestPath, 'utf8'));
pkg.contributes = pkg.contributes || {};

const current = JSON.stringify(pkg.contributes.keybindings || []);
const next = JSON.stringify(keybindings);

if (checkOnly) {
  if (current !== next) {
    console.error('contributes.keybindings is stale; run node clients/vscode/scripts/gen-keybindings.mjs');
    process.exit(1);
  }
  console.log(`ok ${keybindings.length} keybindings`);
  process.exit(0);
}

pkg.contributes.keybindings = keybindings;
writeFileSync(manifestPath, `${JSON.stringify(pkg, null, 2)}\n`);
console.log(`wrote ${keybindings.length} keybindings to ${manifestPath}`);
