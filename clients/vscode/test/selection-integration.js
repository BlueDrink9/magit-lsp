"use strict";

// Scripted editor evidence, NOT native VSCode evidence. Shape tests intercept
// transport; Git tests below use the real server without interception.
const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

/** Install an ordered, native-equivalent anchor/active Selection. */
function select(vscode, editor, anchor, active) {
  for (const [line, column] of [anchor, active]) {
    assert(line >= 0 && line < editor.document.lineCount, "valid document row");
    assert(
      column >= 0 && column <= editor.document.lineAt(line).text.length,
      "valid document column",
    );
  }
  editor.selection = new vscode.Selection(
    new vscode.Position(...anchor),
    new vscode.Position(...active),
  );
}

/** Assert one fresh registered-dispatch request and its exact selection/cursor. */
function checkRequest(records, before, method, active, rows) {
  const sent = records
    .slice(before)
    .filter((r) => r.method === "view/key" || r.method === "view/invoke");
  assert.equal(sent.length, 1, "exactly one fresh dispatch (no stale record)");
  const request = sent[0];
  assert.equal(request.method, method);
  assert.deepEqual(request.params.cursor, { line: active[0], column: active[1] });
  if (rows) {
    assert.deepEqual(request.params.selection, { startLine: rows[0], endLine: rows[1] });
  } else {
    assert.equal(Object.hasOwn(request.params, "selection"), false, "caret omits selection");
  }
  if (method === "view/key") assert.equal(request.params.editorMode, rows ? "visual" : "normal");
  else assert.equal(Object.hasOwn(request.params, "editorMode"), false);
  return request;
}

/** Exercise document-valid first/last/EOF boundaries through both dispatchers. */
async function checkShapes({ vscode, commands, client, editor }) {
  const originalSendRequest = client.sendRequest;
  const originalDocument = editor.document;
  const originalSelection = editor.selection;
  const viewId = require("../lib/store").store.activeViewId;
  const originalDocuments = new Map(vscode.__test.documents);
  const records = [];
  try {
    client.sendRequest = function sendRequest(method, params) {
      if (method === "view/key" || method === "view/invoke") {
        records.push({ method, params: structuredClone(params) });
        return Promise.resolve({ effects: [] });
      }
      return originalSendRequest.call(this, method, params);
    };
    for (const newline of [false, true]) {
      const document = await vscode.workspace.openTextDocument({
        content: `first\nmiddle\nlast${newline ? "\n" : ""}`,
      });
      document.uri = originalDocument.uri;
      editor.document = document;
      const eof = newline ? [3, 0] : [2, 4];
      assert.equal(document.lineCount, newline ? 4 : 3);
      const cases = [
        ["column-zero exclusive", [0, 1], [2, 0], [0, 1]],
        ["nonzero inclusive", [0, 1], [2, 1], [0, 2]],
        ["same-line partial", [1, 1], [1, 4], [1, 1]],
        ["first full row", [0, 0], [1, 0], [0, 0]],
        ["last content row to EOF", [2, 0], eof, [2, 2]],
        ["whole document to EOF", [0, 0], eof, [0, 2]],
        ["EOF caret", eof, eof, null],
        ["first caret", [0, 0], [0, 0], null],
        ["middle caret", [1, 3], [1, 3], null],
      ];
      for (const [name, start, end, rows] of cases) {
        for (const reverse of [false, true]) {
          const anchor = reverse ? end : start;
          const active = reverse ? start : end;
          for (const method of ["view/key", "view/invoke"]) {
            select(vscode, editor, anchor, active);
            assert.deepEqual(editor.selection.start, new vscode.Position(...start), name);
            assert.deepEqual(editor.selection.end, new vscode.Position(...end), name);
            const before = records.length;
            if (method === "view/key") await commands.get("magit.key")({ key: "x" });
            else await commands.get("magit.invoke")({ command: "common.refresh" });
            const request = checkRequest(records, before, method, active, rows);
            assert.equal(request.params.viewId, viewId);
            assert.equal(
              request.params[method === "view/key" ? "key" : "command"],
              method === "view/key" ? "x" : "common.refresh",
            );
          }
        }
      }
    }
  } finally {
    client.sendRequest = originalSendRequest;
    editor.document = originalDocument;
    editor.selection = originalSelection;
    vscode.__test.documents.clear();
    for (const [uri, document] of originalDocuments) vscode.__test.documents.set(uri, document);
  }
}

/** Run Git only in the isolated fixture and return exact bytes. */
function git(repo, ...args) {
  return execFileSync("git", ["-C", repo, ...args]);
}

// The five surviving range-axis values. `context/0` is the only context-row
// cursor paired with a column-zero exclusive endpoint; `changed/0` is the
// changed-row equivalent; `/1` includes SECOND; `eof/0` is the same-line
// hunk/EOF edge with no later row to include.
const RANGE_CURSORS = ["context/0", "context/1", "changed/0", "changed/1", "eof/0"];

/**
 * Map one range-axis value to its forward cursor, exclusive endpoint, selected
 * rows and the changed fixture lines the selection covers.
 */
function rangeSelection(range, first, last, document) {
  const [position, nonzero] = range.split("/");
  assert(["context", "changed", "eof"].includes(position), `known range cursor: ${range}`);
  assert(["0", "1"].includes(nonzero), `known endpoint mode: ${range}`);
  if (position === "eof") {
    assert.equal(nonzero, "0", "same-line EOF selection has no later row");
    return {
      start: [last, 0],
      end: [last, document.lineAt(last).text.length],
      rows: [last, last],
      selected: ["GUARD"],
    };
  }
  const start = [position === "context" ? first - 1 : first, 0];
  return {
    start,
    end: [first + 1, nonzero === "1" ? 1 : 0],
    rows: [start[0], nonzero === "1" ? first + 1 : first],
    selected: nonzero === "1" ? ["FIRST", "SECOND"] : ["FIRST"],
  };
}

/**
 * Enumerate the blessed 64-case selection matrix: the 60-case core product
 * deletion(2) × ending(3) × unstage(2) × range(5) at baseline dispatch
 * (registered key, forward selection), plus two dedicated alternate-method
 * cases spanning both stage and unstage payloads, plus two dedicated reverse
 * cases spanning the context-start exclusive and changed-row inclusive cursors.
 */
function selectionCases() {
  const cases = [];
  for (const deletion of [false, true]) {
    for (const ending of ["interior", "newline", "no-newline"]) {
      for (const unstage of [false, true]) {
        for (const range of RANGE_CURSORS) {
          cases.push({ deletion, ending, unstage, range, method: "view/key", reverse: false });
        }
      }
    }
  }
  // Alternate dispatcher for both key/command payloads: `s`/`status.stage`
  // and `u`/`status.unstage`.
  cases.push({
    deletion: false,
    ending: "interior",
    unstage: false,
    range: "changed/1",
    method: "view/invoke",
    reverse: false,
    focus: "stage invoke",
  });
  cases.push({
    deletion: true,
    ending: "newline",
    unstage: true,
    range: "context/1",
    method: "view/invoke",
    reverse: false,
    focus: "unstage invoke",
  });
  // Swapped anchor/active at both cursor positions and endpoint modes.
  cases.push({
    deletion: false,
    ending: "no-newline",
    unstage: false,
    range: "context/0",
    method: "view/key",
    reverse: true,
    focus: "context-start reverse",
  });
  cases.push({
    deletion: true,
    ending: "interior",
    unstage: false,
    range: "changed/1",
    method: "view/key",
    reverse: true,
    focus: "changed-row reverse",
  });
  assert.equal(cases.length, 64, "blessed 64-case selection matrix");
  // Every original axis keeps coverage after the collapse.
  for (const deletion of [false, true])
    assert(
      cases.some((c) => c.deletion === deletion),
      `deletion=${deletion} covered`,
    );
  for (const ending of ["interior", "newline", "no-newline"])
    assert(
      cases.some((c) => c.ending === ending),
      `ending=${ending} covered`,
    );
  for (const unstage of [false, true])
    assert(
      cases.some((c) => c.unstage === unstage),
      `unstage=${unstage} covered`,
    );
  for (const range of RANGE_CURSORS)
    assert(
      cases.some((c) => c.range === range),
      `range=${range} covered`,
    );
  for (const method of ["view/key", "view/invoke"])
    assert(
      cases.some((c) => c.method === method),
      `method=${method} covered`,
    );
  for (const reverse of [false, true])
    assert(
      cases.some((c) => c.reverse === reverse),
      `reverse=${reverse} covered`,
    );
  for (const unstage of [false, true])
    assert(
      cases.some((c) => c.method === "view/invoke" && c.unstage === unstage),
      `alternate method covers unstage=${unstage} payload`,
    );
  assert(
    cases.some((c) => c.reverse && c.range === "context/0"),
    "reverse covers the context-start exclusive cursor",
  );
  assert(
    cases.some((c) => c.reverse && c.range === "changed/1"),
    "reverse covers the changed-row inclusive cursor",
  );
  return cases;
}

/** Stage/unstage real rendered ranges via registered commands and real RPC. */
async function checkGitSelections({ vscode, commands, client }) {
  const messageStart = vscode.__test.messages.length;
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), "magit-selection-"));
  const file = path.join(repo, "boundary.txt");
  const originalEditor = vscode.__test.activeEditor;
  const originalWorkspace = require("../lib/store").store.views.get(
    require("../lib/store").store.activeViewId,
  )?.repo;
  let opened = false;
  let count = 0;
  try {
    git(repo, "init", "-q");
    git(repo, "config", "user.name", "Test");
    git(repo, "config", "user.email", "test@example.com");
    fs.writeFileSync(file, "initial\n");
    git(repo, "add", ".");
    git(repo, "commit", "-qm", "initial");
    vscode.__test.setWorkspaceFolder(repo);
    await commands.get("magit.open")();
    opened = true;
    const editor = vscode.__test.activeEditor;
    const fixtureState = require("../lib/store").store.views.get(
      require("../lib/store").store.activeViewId,
    );
    assert.equal(fixtureState.repo, repo, "registered open focused the fixture repository");
    // Status sections open collapsed (src/status_view.rs:99-107), so unfold
    // them once: the matrix addresses file and hunk rows through the
    // acknowledged tree, which a closed section does not publish. Wait for the
    // fresh render first — an invoke during the deferred acquisition reports
    // against the loading tree, whose sections are not yet in the view.
    const sectionsDeadline = Date.now() + 10000;
    while (!fixtureState.lines.some((line) => line.startsWith("Unstaged changes"))) {
      assert(Date.now() < sectionsDeadline, "fresh status sections rendered");
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    await commands.get("magit.invoke")({ command: "common.show-all" });
    // Each fixture has three adjacent changed lines. The last is always a
    // guard: colzero excludes SECOND; nonzero includes SECOND but not GUARD.
    const cases = selectionCases();
    for (const deletion of [false, true]) {
      for (const ending of ["interior", "newline", "no-newline"]) {
        const prefix = "head\ncontext\n";
        const suffix = ending === "interior" ? "\ntail\n" : ending === "newline" ? "\n" : "";
        const changed = `${prefix}FIRST\nSECOND\nGUARD${suffix}`;
        const unchanged = ending === "interior" ? `${prefix}tail\n` : prefix;
        const base = deletion ? changed : unchanged;
        const work = deletion ? unchanged : changed;
        fs.writeFileSync(file, base);
        git(repo, "add", "boundary.txt");
        git(repo, "commit", "--allow-empty", "-qm", `${deletion ? "delete" : "add"} ${ending}`);
        for (const selectionCase of cases) {
          if (selectionCase.deletion !== deletion || selectionCase.ending !== ending) continue;
          const { unstage, method, reverse, range } = selectionCase;
          fs.writeFileSync(file, base);
          git(repo, "add", "boundary.txt");
          fs.writeFileSync(file, work);
          if (unstage) git(repo, "add", "boundary.txt");
          editor.selection = new vscode.Selection(
            new vscode.Position(0, 0),
            new vscode.Position(0, 0),
          );
          await commands.get("magit.invoke")({ command: "common.refresh" });
          // Symbols come from the acknowledged server tree. Never
          // discover hunk rows by parsing rendered text.
          let files;
          const deadline = Date.now() + 10000;
          do {
            const symbols = await client.sendRequest("textDocument/documentSymbol", {
              textDocument: { uri: fixtureState.descriptor.uri },
            });
            files = symbols
              .filter((s) => s.name === (unstage ? "Staged changes" : "Unstaged changes"))
              .flatMap((s) => s.children || []);
            const candidate = files[0]?.children?.[0];
            const firstRow = candidate?.range.start.line + 3;
            const marker = deletion ? "-" : "+";
            // Render acknowledgements are asynchronous. Validate the
            // known fixture at metadata-derived rows before using it;
            // neither names nor rendered text discover row positions.
            if (
              files.length === 1 &&
              candidate &&
              ["FIRST", "SECOND", "GUARD"].every(
                (line, offset) =>
                  editor.document.lineAt(firstRow + offset).text === `    ${marker}${line}`,
              ) &&
              candidate.range.end.line >= firstRow + 2
            )
              break;
            assert(Date.now() < deadline, "fresh acknowledged fixture symbols");
            await new Promise((resolve) => setTimeout(resolve, 10));
          } while (true);
          assert.equal(files.length, 1, "one structured fixture file");
          const symbol = files[0];
          assert.equal(symbol.kind, 1, "structured file kind");
          assert.equal(symbol.children.length, 1, "one structured hunk");
          const hunk = symbol.children[0];
          const first = hunk.range.start.line + 3; // header + two fixture context lines
          const last = first + 2;
          const marker = deletion ? "-" : "+";
          assert.equal(editor.document.lineAt(first).text, `    ${marker}FIRST`);
          assert.equal(editor.document.lineAt(first + 1).text, `    ${marker}SECOND`);
          assert.equal(editor.document.lineAt(last).text, `    ${marker}GUARD`);
          const { start, end, rows, selected } = rangeSelection(
            range,
            first,
            last,
            editor.document,
          );
          const anchor = reverse ? end : start;
          const active = reverse ? start : end;
          select(vscode, editor, anchor, active);
          const before = client.requests.length;
          const command = unstage ? "status.unstage" : "status.stage";
          if (method === "view/key") await commands.get("magit.key")({ key: unstage ? "u" : "s" });
          else await commands.get("magit.invoke")({ command });
          const request = checkRequest(client.requests, before, method, active, rows);
          assert.equal(request.params.viewId, fixtureState.id);
          assert.equal(
            request.params[method === "view/key" ? "key" : "command"],
            method === "view/key" ? (unstage ? "u" : "s") : command,
          );
          const retained = ["FIRST", "SECOND", "GUARD"].filter((line) => {
            const applied = unstage ? !selected.includes(line) : selected.includes(line);
            return deletion ? !applied : applied;
          });
          // No-newline belongs to GUARD, not to whichever line becomes
          // last after partial staging. Other original lines keep LF.
          const retainedSuffix =
            ending === "no-newline" && !retained.includes("GUARD") ? "\n" : suffix;
          const expected = retained.length
            ? `${prefix}${retained.join("\n")}${retainedSuffix}`
            : unchanged;
          const label = `${deletion ? "deletion" : "addition"} ${ending} ${command} ${method} reverse=${reverse} range=${range}${selectionCase.focus ? ` (${selectionCase.focus})` : ""}`;
          assert.deepEqual(
            git(repo, "show", ":boundary.txt"),
            Buffer.from(expected),
            `${label}: exact index (including guards)`,
          );
          assert.deepEqual(
            fs.readFileSync(file),
            Buffer.from(work),
            `${label}: exact untouched worktree`,
          );
          assert.equal(
            vscode.__test.messages
              .slice(messageStart)
              .filter((m) => m.level === "warning" || m.level === "error").length,
            0,
            label,
          );
          count += 1;
        }
      }
    }
    console.log(
      `ok real-Git selection dispatch (${count} exact index/worktree cases; mocked editor)`,
    );
  } finally {
    try {
      if (opened) {
        // Mutations never replace transport; cleanup still uses registered RPC.
        await commands.get("magit.invoke")({ command: "common.quit" });
      }
    } finally {
      if (originalWorkspace) vscode.__test.setWorkspaceFolder(originalWorkspace);
      if (originalEditor) await vscode.window.showTextDocument(originalEditor.document);
      fs.rmSync(repo, { recursive: true, force: true });
    }
  }
}

module.exports = { checkShapes, checkGitSelections };
