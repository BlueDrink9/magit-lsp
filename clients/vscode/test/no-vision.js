'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

/** Reject executable image capture/snapshot paths while allowing text snapshots. */
module.exports = function noVisionPolicy() {
  const root = path.join(__dirname, 'e2e');
  const files = [];
  const visit = dir => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) visit(file);
      else if (/\.m?js$/.test(entry.name)) files.push(file);
    }
  };
  visit(root);
  for (const file of files) {
    const source = fs.readFileSync(file, 'utf8');
    assert(!/\b(?:Page\.)?captureScreenshot\b|\b(?:writeFileSync|writeFile|readFileSync|readFile)\s*\([^\n]*\.(?:png|jpe?g|webp)\b/i.test(source), `${file} contains image capture/artifact I/O`);
    assert(!/import\s*\(\s*['"](?:sharp|canvas|playwright)['"]/.test(source), `${file} imports an image capture tool`);
  }
  for (const method of ['Page.captureScreenshot', 'Page.snapshotRect', 'DOMSnapshot.captureSnapshot']) {
    const refused = spawnSync(process.execPath, [path.join(root, 'run.mjs'), '--cdp', '1', method, '{}'], { encoding: 'utf8' });
    assert.notEqual(refused.status, 0, `${method} unexpectedly succeeded`);
    assert.match(refused.stderr, /Image-related CDP method is forbidden/, `${method} was not refused by policy before connection`);
  }
};
