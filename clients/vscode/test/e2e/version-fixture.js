'use strict';

const fs = require('node:fs');
const path = require('node:path');

const source = path.resolve(__dirname, '../..');

/** Create isolated extension copy with a sentinel package version. */
function createVersionFixture(parent) {
  const extensionPath = path.join(parent, 'version-extension');
  fs.mkdirSync(extensionPath, { recursive: true });
  const copyRuntime = (from, to) => {
    fs.mkdirSync(to, { recursive: true });
    for (const entry of fs.readdirSync(from, { withFileTypes: true })) {
      const sourcePath = path.join(from, entry.name);
      const targetPath = path.join(to, entry.name);
      if (entry.isDirectory()) copyRuntime(sourcePath, targetPath);
      else if (entry.isFile() && entry.name.endsWith('.js')) fs.copyFileSync(sourcePath, targetPath);
    }
  };
  copyRuntime(path.join(source, 'lib'), path.join(extensionPath, 'lib'));
  fs.copyFileSync(path.join(source, 'extension.js'), path.join(extensionPath, 'extension.js'));
  const originalManifest = JSON.parse(fs.readFileSync(path.join(source, 'package.json'), 'utf8'));
  const manifest = { ...originalManifest, version: '9999.0.0' };
  fs.writeFileSync(path.join(extensionPath, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  fs.symlinkSync(path.join(source, 'node_modules'), path.join(extensionPath, 'node_modules'), 'dir');
  return {
    extensionPath,
    cleanup() { fs.rmSync(extensionPath, { recursive: true, force: true }); },
  };
}

module.exports = { createVersionFixture };
