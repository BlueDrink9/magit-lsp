"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");

/** Verify native root quit cleanup and reopening without disturbing another root. */
async function verifyQuitReopen({
  extension,
  evidence,
  repo,
  uri,
  requests,
  statusTabs,
  checkpoint,
  quickInputState,
  waitFor,
  vscode,
  execFileSync,
  path,
  settle,
  windowId,
}) {
  const { store } = require(`${extension.extensionPath}/lib/store`);
  const views = require(`${extension.extensionPath}/lib/views`);
  const otherRepo = path.join(evidence, "other-repo");
  execFileSync("git", ["clone", "-q", repo, otherRepo]);
  const descriptor = await store.client.sendRequest("view/open", {
    kind: "status",
    args: { repo_path: otherRepo },
    viewport: { cols: 100, rows: 30 },
  });
  const other = views.ensureView(descriptor);
  other.rootId = other.id;
  other.repo = otherRepo;
  await views.showView(descriptor);
  const root = views.stateForUri(uri);
  await views.focusView(root.id);
  await vscode.commands.executeCommand("magit.invoke", { command: "open.commit" });
  await waitFor(() => quickInputState().visible, "nested menu visible");
  await vscode.commands.executeCommand("magit.invoke", { command: "common.quit" });
  await waitFor(() => !quickInputState().visible, "nested menu popped");
  assert(store.views.has(root.id), "nested quit removed root");
  await views.focusView(root.id);
  await vscode.commands.executeCommand("magit.invoke", { command: "open.commit" });
  await waitFor(
    () => quickInputState().visible && /Commit/i.test(quickInputState().text || ""),
    "native commit QuickPick",
  );
  checkpoint("a06-menu-before-quit.json");
  const removed = [...store.views.values()].filter((state) => state.rootId === root.id);
  const beforeClose = requests.filter((request) => request.method === "view/close").length;
  // Quitting the menu only pops it; target the root with that menu open.
  await require(`${extension.extensionPath}/lib/requests`).invoke(
    root.id,
    "common.quit",
    undefined,
    { cursor: { line: 0, column: 0 } },
  );
  await waitFor(
    () =>
      removed.every(
        (state) => !statusTabs().some((tab) => tab.input.uri.toString() === state.uri.toString()),
      ),
    "root-owned tabs closed",
  );
  await waitFor(() => !quickInputState().visible, "QuickPick dismissed");
  assert.equal(
    requests.filter((request) => request.method === "view/close").length,
    beforeClose,
    "server quit did not recursively request view/close",
  );
  for (const state of removed) {
    assert(!store.views.has(state.id), "quit retained view");
    assert(!store.uriToView.has(state.uri.toString()), "quit retained URI");
    assert(!store.pendingRenders.has(state.id), "quit retained pending render");
    assert(!state.quickPick, "quit retained QuickPick");
  }
  assert(store.views.has(other.id), "quit removed unrelated root");
  assert(
    statusTabs().some((tab) => tab.input.uri.toString() === other.uri.toString()),
    "quit closed unrelated tab",
  );
  await vscode.window.showTextDocument(vscode.Uri.file(path.join(repo, "tracked.txt")));
  await waitFor(() => store.activeViewId === undefined, "inactive view context");
  const afterQuitCount = requests.length;
  await vscode.commands.executeCommand("magit.key", { key: "g" });
  await settle(300);
  assert.equal(requests.length, afterQuitCount, "inactive contexts dispatched a Magit key");
  checkpoint("a06-quit.json");
  await vscode.commands.executeCommand("magit.open");
  await waitFor(() => statusTabs().length === 2, "status reopened alongside other root");
  const reopened = vscode.window.activeTextEditor;
  assert(reopened && reopened.document.uri.scheme === "magit");
  await waitFor(
    () => reopened.document.getText().includes("tracked.txt"),
    "reopened status render",
  );
  checkpoint("a06-reopened.json");
  const refreshCount = requests.length;
  execFileSync("xdotool", ["key", "--window", windowId, "--clearmodifiers", "g"]);
  await waitFor(
    () =>
      requests
        .slice(refreshCount)
        .some(
          (request) =>
            request.method === "view/invoke" && request.params.command === "common.refresh",
        ),
    "reopened status responds to physical refresh",
  );
  fs.writeFileSync(
    `${evidence}/a06-evidence.json`,
    JSON.stringify(
      {
        menuBeforeQuit: "a06-menu-before-quit.json",
        quit: "a06-quit.json",
        reopened: "a06-reopened.json",
        affectedTabsAfterQuit: 0,
        preservedRoot: other.id,
        removedViews: removed.map((state) => state.id),
        reopenedRoot: views.activeState().rootId,
        reopenedRefresh: requests.slice(refreshCount),
        quickPickHiddenAfterQuit: true,
        noRecursiveViewClose: true,
        keyDispatchedAfterQuit: false,
      },
      null,
      2,
    ),
  );
  console.log("PASS native A06 root quit closes menu/tabs and reopens status");
}

module.exports = { verifyQuitReopen };
