"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

/**
 * Read the isolated fixture's HEAD, index entries and worktree bytes without
 * mutating it. `git status` may refresh `.git/index` stat data, so the index is
 * compared by its recorded entries rather than the raw file.
 *
 * @param {string} repo
 * @returns {object}
 */
function fixtureSnapshot(repo) {
  return {
    head: execFileSync("git", ["-C", repo, "rev-parse", "HEAD"]).toString().trim(),
    index: execFileSync("git", ["-C", repo, "ls-files", "-s", "-z"]).toString(),
    status: execFileSync("git", [
      "-C",
      repo,
      "status",
      "--porcelain=v1",
      "-z",
      "--untracked-files=all",
    ]).toString(),
    worktree: Object.fromEntries(
      ["tracked.txt", ".vscode/settings.json"].map((relative) => [
        relative,
        fs.readFileSync(path.join(repo, relative)).toString("base64"),
      ]),
    ),
  };
}

/**
 * Decode the server's full-document semantic tokens into absolute ranges.
 *
 * @param {{data: number[]}} tokens
 * @param {{tokenTypes: string[]}} legend
 * @returns {Array<{line: number, start: number, length: number, type: string}>}
 */
function decodeTokens(tokens, legend) {
  const entries = [];
  let line = 0;
  let start = 0;
  for (let index = 0; index < tokens.data.length; index += 5) {
    line += tokens.data[index];
    start = tokens.data[index] === 0 ? start + tokens.data[index + 1] : tokens.data[index + 1];
    entries.push({
      line,
      start,
      length: tokens.data[index + 2],
      type: legend.tokenTypes[tokens.data[index + 3]],
    });
  }
  return entries;
}

/**
 * Verify native VSCode paints the server's canonical added and removed roles
 * with distinct computed foregrounds on the actual diff content cells, without
 * changing the isolated fixture or switching themes.
 *
 * @param {object} context suite-owned helpers and the focused status view
 */
async function verifySemanticRoles({
  vscode,
  extension,
  editor,
  uri,
  repo,
  evidence,
  renderedLines,
  semanticRequests,
  checkpoint,
  waitFor,
  settle,
  cdp,
}) {
  const before = fixtureSnapshot(repo);
  // The editor document URI is a client identity; the provider translates it
  // through the extension's own state map before pulling server tokens.
  const views = require(`${extension.extensionPath}/lib/views`);
  const state = views.stateForUri(uri);
  assert(state, `the focused editor URI has no client view state: ${uri.toString()}`);
  const clientUri = uri.toString();
  const serverUri = state.descriptor.uri;
  const theme = vscode.workspace.getConfiguration("workbench").get("colorTheme");
  // The workbench reports the scheme label ("Dark Modern") or its ID
  // ("Default Dark Modern"); both name the configured default dark scheme.
  assert(
    /^(Default )?Dark Modern$/.test(theme || ""),
    `focused smoke requires the default dark scheme, got ${theme}`,
  );

  await vscode.commands.executeCommand("magit.invoke", { command: "common.show-all", args: {} });
  await waitFor(() => editor.document.getText().includes("+replacement"), "expanded status diff");
  const statusText = editor.document.getText();
  assert(statusText.includes("-original"), "the real status diff lost its removed content line");
  assert(statusText.includes("+replacement"), "the real status diff lost its added content line");
  assert.match(statusText, /tracked\.txt/, "the real status diff lost its file path");

  const legend = await vscode.commands.executeCommand(
    "vscode.provideDocumentSemanticTokensLegend",
    uri,
  );
  const roles = JSON.parse(
    fs.readFileSync(path.resolve(__dirname, "../../../..", "src/semantic_roles.json"), "utf8"),
  );
  assert.deepEqual(
    legend.tokenTypes,
    roles.map((role) => role.name),
    "the host legend is not the canonical role table",
  );
  let tokens;
  await waitFor(async () => {
    tokens = await vscode.commands.executeCommand("vscode.provideDocumentSemanticTokens", uri);
    return tokens?.data.length > 0;
  }, "semantic tokens");
  assert(
    semanticRequests.some((request) => request.params?.textDocument?.uri === serverUri),
    `the extension host never requested semantic tokens for the focused view ${serverUri}`,
  );

  const entries = decodeTokens(tokens, legend);
  const lines = editor.document.getText().split("\n");
  const pick = (type, needle) => {
    const entry = entries.find(
      (candidate) => candidate.type === type && (lines[candidate.line] || "").includes(needle),
    );
    assert(entry, `no server ${type} token covers the expected diff text ${needle}`);
    return entry;
  };
  const wanted = [
    { role: "added", entry: pick("added", "+replacement"), content: "replacement" },
    { role: "removed", entry: pick("removed", "-original"), content: "original" },
  ].map(({ role, entry, content }) => {
    const lineText = lines[entry.line] || "";
    const offset = lineText.indexOf(content);
    assert(offset >= 0, `${role} token line does not contain ${content}`);
    assert(
      offset >= entry.start && offset + content.length <= entry.start + entry.length,
      `${role} token range does not cover the ${content} content cell`,
    );
    return {
      role,
      line: entry.line,
      start: entry.start,
      length: entry.length,
      lineText,
      offset,
      content,
    };
  });
  editor.revealRange(
    new vscode.Range(wanted[0].line, 0, wanted[0].line, 0),
    vscode.TextEditorRevealType.InCenter,
  );
  await settle(150);

  const measured = cdp("Runtime.evaluate", {
    expression: `(() => {
      const wanted = ${JSON.stringify(wanted)};
      const normalize = (text) => text.replace(/\\u00a0/g, ' ');
      const viewLines = [...document.querySelectorAll('.view-line')];
      const lineTexts = viewLines.map((element) => normalize(element.textContent));
      const editorElement = document.querySelector('.monaco-editor');
      const style = editorElement && getComputedStyle(editorElement);
      const tokens = wanted.map((token) => {
        const index = lineTexts.indexOf(token.lineText);
        if (index < 0) return { ...token, found: false, cellFound: false };
        const lineElement = viewLines[index];
        const cells = [];
        const walker = document.createTreeWalker(lineElement, NodeFilter.SHOW_TEXT);
        let offset = 0;
        let node;
        while ((node = walker.nextNode())) {
          const text = normalize(node.nodeValue);
          for (let position = 0; position < text.length; position += 1) {
            cells.push({ element: node.parentElement, at: offset + position });
          }
          offset += text.length;
        }
        const cell = cells[token.offset];
        const cellStyle = cell && getComputedStyle(cell.element);
        return { ...token, found: true, cellFound: Boolean(cell),
          text: token.lineText.slice(token.offset, token.offset + token.content.length),
          className: cell && cell.element.className,
          color: cellStyle && cellStyle.color,
          backgroundColor: cellStyle && cellStyle.backgroundColor,
          fontWeight: cellStyle && cellStyle.fontWeight };
      });
      return {
        defaultColor: style && style.color,
        background: style && style.backgroundColor,
        themeClass: document.querySelector('.monaco-workbench')?.className,
        tokens,
      };
    })()`,
    returnByValue: true,
  }).result.value;
  assert.equal(
    editor.document.getText(),
    (renderedLines.get(uri.authority) || []).join("\n"),
    "colour measurement changed the server-owned text",
  );

  const byRole = Object.fromEntries(measured.tokens.map((token) => [token.role, token]));
  for (const role of ["added", "removed"]) {
    const token = byRole[role];
    assert(token.found, `${role} token line is not rendered`);
    assert(token.cellFound, `${role} content cell is not rendered`);
    assert.equal(token.text, token.content, `${role} cell does not hold the expected content`);
    assert(
      /^rgba?\(/.test(token.color || ""),
      `${role} cell has no computed foreground: ${token.color}`,
    );
    assert.notEqual(
      token.color,
      measured.defaultColor,
      `${role} cell merely inherited the default foreground ${token.color}`,
    );
  }
  assert.notEqual(
    byRole.added.color,
    byRole.removed.color,
    `added and removed content share foreground ${byRole.added.color}`,
  );

  const checkpointPath = checkpoint("t028-semantic-roles.json");
  const after = fixtureSnapshot(repo);
  assert.deepEqual(after, before, "the focused semantic smoke changed the isolated fixture");
  const evidencePath = `${evidence}/t028-semantic-roles-evidence.json`;
  fs.writeFileSync(
    evidencePath,
    JSON.stringify(
      {
        status: "pass",
        gate: "T028",
        editorVersion: vscode.version,
        clientUri,
        serverUri,
        viewId: state.descriptor.id,
        uri: clientUri,
        scheme: theme,
        legend,
        canonicalRoles: roles.map((role) => role.name),
        semanticTokenRequests: semanticRequests,
        expectedDiff: ["-original", "+replacement", "tracked.txt"],
        measured,
        assertions: {
          rolesPresent: ["added", "removed"],
          addedColor: byRole.added.color,
          removedColor: byRole.removed.color,
          distinct: byRole.added.color !== byRole.removed.color,
          defaultColor: measured.defaultColor,
        },
        fixture: { before, after, unchanged: true },
        checkpoints: [checkpointPath],
        limitations:
          "One default dark scheme (Default Dark Modern) on the real Linux " +
          "VSCode host; no theme matrix or scheme transition. Foregrounds are computed " +
          "CSS colours of the rendered content cells, not screenshots. Process cleanup " +
          "evidence covers only processes whose command lines name the isolated " +
          "profile/workspace paths; the shared-build server and an extension host " +
          "without those paths are a blind spot that check cannot observe.",
      },
      null,
      2,
    ),
  );
  await settle(100);
  console.log(
    `PASS native T028 semantic roles: added ${byRole.added.color}, ` +
      `removed ${byRole.removed.color}; fixture unchanged; ${evidencePath}`,
  );
}

module.exports = { verifySemanticRoles };
