'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

/** Requests this test records as evidence; notifications are not captured. */
const WATCHED = new Set(['view/close', 'view/invoke', 'view/key', 'view/open',
  'workspace/executeCommand']);

/**
 * Resolve after `milliseconds`, only used to let a native widget mount.
 *
 * The test still polls observable state for every assertion; this is not a
 * substitute for waiting on a condition.
 *
 * @param {number} milliseconds
 */
function settle(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/**
 * Poll `condition` until it is truthy or the timeout elapses.
 *
 * @param {() => unknown|Promise<unknown>} condition
 * @param {string} label
 * @param {number} [timeout]
 */
async function waitFor(condition, label, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (!(await condition())) {
    if (Date.now() > deadline) throw new Error(`Timed out waiting for ${label}`);
    await settle(30);
  }
}

/**
 * List the workspace's files, excluding Git metadata, to prove no backing
 * status document is created by opening or refreshing a view.
 *
 * @param {string} root
 * @returns {string[]}
 */
function workspaceFiles(root) {
  const files = [];
  const walk = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      if (entry.name === '.git') continue;
      const full = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(full);
      else files.push(path.relative(root, full));
    }
  };
  walk(root);
  return files.sort();
}

/** Verify first-open/reopen/refresh plus foreign-widget isolation and status keys. */
async function run() {
  const evidence = process.env.MAGIT_VSCODE_EVIDENCE;
  assert(evidence, 'MAGIT_VSCODE_EVIDENCE must name the isolated evidence directory');
  const extension = vscode.extensions.getExtension('magit-lsp.magit-lsp');
  const requests = [];
  const draftNotes = [];
  const draftMessages = [];
  const progressEvents = [];
  const sendNotification = LanguageClient.prototype.sendNotification;
  LanguageClient.prototype.sendNotification = function (method, params, ...rest) {
    const name = typeof method === 'string' ? method : method.method;
    if (params?.textDocument?.uri?.endsWith('/COMMIT_EDITMSG') || name === 'window/workDoneProgress/cancel') {
      draftNotes.push({ method: name, params });
    }
    return sendNotification.call(this, method, params, ...rest);
  };
  const onProgress = LanguageClient.prototype.onProgress;
  LanguageClient.prototype.onProgress = function (type, token, handler) {
    return onProgress.call(this, type, token, (value) => {
      progressEvents.push({ token, value });
      return handler(value);
    });
  };
  /** Server semantic-token pulls, kept separate from Magit view traffic. */
  const semanticRequests = [];
  const openedViews = [];
  const renderedLines = new Map();
  const renderEvents = [];
  const recordRender = (params) => {
    if (!params || !params.viewId) return;
    let next = renderedLines.get(params.viewId) || [];
    if (Array.isArray(params.text)) {
      next = params.text.slice();
    } else if (Array.isArray(params.edits)) {
      next = next.slice();
      for (const edit of params.edits
        .slice()
        .sort((left, right) => right.startLine - left.startLine)) {
        const start = Math.max(0, edit.startLine);
        const end = Math.max(start, edit.endLine);
        next.splice(start, end - start, ...(edit.lines || []));
      }
    }
    renderedLines.set(params.viewId, next);
    renderEvents.push(params);
  };
  const sendRequest = LanguageClient.prototype.sendRequest;
  LanguageClient.prototype.sendRequest = function (method, params, ...rest) {
    if (WATCHED.has(method)) requests.push({ method, params });
    // The semantic provider asks the client for the server's own tokens; this
    // is evidence for R-VS-4 and must not inflate Magit view traffic counts.
    if (method === 'textDocument/semanticTokens/full') {
      semanticRequests.push({ method, params });
    }
    const result = sendRequest.call(this, method, params, ...rest);
    if (process.env.MAGIT_VSCODE_HEAD_ONLY === '1' && ['view/key', 'view/invoke'].includes(method)) {
      return Promise.resolve(result).then((response) => {
        draftMessages.push(...(response?.effects || []).filter((effect) => effect.type === 'message'));
        return response;
      });
    }
    if (method === 'view/open') {
      return Promise.resolve(result).then((descriptor) => {
        openedViews.push(descriptor);
        return descriptor;
      });
    }
    if (method === 'view/resync') {
      return Promise.resolve(result).then((render) => {
        recordRender(render);
        return render;
      });
    }
    return result;
  };
  const onNotification = LanguageClient.prototype.onNotification;
  LanguageClient.prototype.onNotification = function (method, handler, ...rest) {
    if (method === 'view/effects') return onNotification.call(this, method, (params) => {
      draftMessages.push(...(params.effects || []).filter((effect) => effect.type === 'message'));
      return handler(params);
    }, ...rest);
    if (method !== 'view/render') {
      return onNotification.call(this, method, handler, ...rest);
    }
    return onNotification.call(this, method, (params) => {
      recordRender(params);
      return handler(params);
    }, ...rest);
  };
  const magitTraffic = () => requests.length;
  const invoked = (command) => requests.some(
    (request) => request.method === 'view/invoke' && request.params.command === command
  );
  const keysSent = () => requests.filter((request) => request.method === 'view/key')
    .map((request) => request.params.key);
  const matrix = [];
  const interactions = [];
  const cdp = (method, params) => JSON.parse(execFileSync('node', [
    `${extension.extensionPath}/test/e2e/run.mjs`, '--cdp',
    process.env.MAGIT_VSCODE_CDP_PORT, method, JSON.stringify(params),
  ], { timeout: 10000, maxBuffer: 16 * 1024 * 1024 }).toString());
  const observe = () => cdp('Runtime.evaluate', {
    expression: `({tag: document.activeElement?.tagName,
      className: document.activeElement?.className,
      editorFocused: Boolean(document.activeElement?.closest('.monaco-editor')),
      aria: document.activeElement?.getAttribute('aria-label'),
      value: document.activeElement?.value,
      findVisible: Boolean(document.querySelector('.find-widget.visible'))})`,
    returnByValue: true,
  }).result.value;
  const shot = (name) => {
    const target = `${evidence}/${name}`;
    fs.writeFileSync(target, Buffer.from(cdp('Page.captureScreenshot', {
      format: 'png', fromSurface: true,
    }).data, 'base64'));
    return target;
  };
  /**
   * Record every physical keydown the isolated renderer sees, with modifier
   * state, so a key that resolves to nothing can be distinguished from a key
   * that never reached the workbench. It observes input and dispatches nothing.
   *
   * @returns {unknown}
   */
  const startKeyProbe = () => cdp('Runtime.evaluate', {
    expression: `(() => {
      window.__magitKeyEvents = [];
      const listener = (event) => {
        window.__magitKeyEvents.push({ key: event.key, code: event.code,
          ctrl: event.ctrlKey, alt: event.altKey, shift: event.shiftKey,
          meta: event.metaKey, repeat: event.repeat });
      };
      window.addEventListener('keydown', listener, true);
      window.__magitKeyProbeStop = () => window.removeEventListener('keydown', listener, true);
      return true;
    })()`,
    returnByValue: true,
  });
  /**
   * Stop the keydown probe and return what it recorded.
   *
   * @returns {object[]}
   */
  const stopKeyProbe = () => {
    const events = cdp('Runtime.evaluate', {
      expression: 'window.__magitKeyEvents',
      returnByValue: true,
    }).result.value || [];
    cdp('Runtime.evaluate', {
      expression: 'window.__magitKeyProbeStop && window.__magitKeyProbeStop()',
      returnByValue: true,
    });
    return events;
  };
  /**
   * Report whether a native quick-input widget is actually painted.
   *
   * @returns {{visible: boolean, text: (string|null)}}
   */
  const quickInputState = () => cdp('Runtime.evaluate', {
    expression: `(() => {
      const widget = document.querySelector('.quick-input-widget');
      if (!widget) return { visible: false, text: null };
      const style = getComputedStyle(widget);
      const visible = style.display !== 'none' && style.visibility !== 'hidden'
        && widget.getBoundingClientRect().height > 0;
      return { visible, text: visible
        ? widget.innerText.replace(/\\s+/g, ' ').slice(0, 120) : null };
    })()`,
    returnByValue: true,
  }).result.value;
  const captureFailure = () => shot('failure-screen.png');
  await extension.activate();
  let windowId;
  let windowGeometry = 'unknown';
  try {
    const registered = new Set(await vscode.commands.getCommands(true));
    for (const { command } of extension.packageJSON.contributes.commands) {
      assert(registered.has(command), `unregistered contribution: ${command}`);
    }

    const repo = vscode.workspace.workspaceFolders[0].uri.fsPath;
    const filesBefore = workspaceFiles(repo);
    // Target only this isolated host window, never a user's existing editor.
    // Give it a deterministic geometry so a small WM tile cannot squeeze out
    // the editor or the find widget between runs.
    await waitFor(() => {
      try {
        const matches = execFileSync('xdotool', ['search', '--name', evidence])
          .toString().trim().split('\n').filter(Boolean);
        if (matches.length !== 1) return false;
        [windowId] = matches;
        return true;
      } catch {
        return false;
      }
    }, 'one isolated host window', 20000);
    // bspwm tiles new windows, which can shrink this isolated host below a
    // usable editor width. Float only the test window so screenshots and the
    // physical-key path use a deterministic geometry.
    try {
      execFileSync('bspc', ['node', windowId, '-t', 'floating'], { stdio: 'ignore' });
    } catch {
      // Non-bspwm hosts: xdotool sizes an already-floating window below.
    }
    execFileSync('xdotool', ['windowsize', '--sync', windowId, '1200', '760']);
    await settle(600);
    windowGeometry = execFileSync(
      'xdotool', ['getwindowgeometry', '--shell', windowId]
    ).toString().trim().split('\n').filter((line) => /^(WIDTH|HEIGHT)=/.test(line)).join(' ');
    await vscode.commands.executeCommand('magit.open');
    const editor = vscode.window.activeTextEditor;
    assert(editor, 'status open did not focus an editor');
    assert.equal(editor.document.uri.scheme, 'magit');
    assert.equal(editor.document.languageId, 'magit');
    const uri = editor.document.uri;
    await waitFor(() => renderedLines.has(uri.authority), 'first server render');
    const first = editor.document.getText();
    const firstRender = (renderedLines.get(uri.authority) || []).join('\n');
    assert(firstRender.trim().length > 0, 'first server render was empty');
    assert(first.split('\n').length > 3, 'first open did not show the status body');
    assert.equal(first, firstRender, 'first-open text does not match the received server render');
    assert.match(first, /tracked\.txt/);
    assert.equal(openedViews.length, 1, 'first open did not return one descriptor');
    assert.equal(openedViews[0].id, uri.authority, 'status URI is not the opened view id');
    assert.equal(openedViews[0].kind, 'status');
    const statusTabs = () => vscode.window.tabGroups.all.flatMap((group) => group.tabs)
      .filter((tab) => tab.input?.uri?.scheme === 'magit');
    assert.equal(statusTabs().length, 1, 'first open did not create exactly one status tab');
    assert.equal(statusTabs()[0].label, openedViews[0].title, 'status tab is not the titled view');
    const statusPath = path.join(repo, decodeURIComponent(uri.path.replace(/^\//, '')));
    assert.equal(fs.existsSync(statusPath), false, 'status view has a backing file on disk');
    assert.deepEqual(workspaceFiles(repo), filesBefore, 'first open created a workspace file');
    await settle(500);
    const firstVisible = vscode.window.visibleTextEditors.find(
      (visible) => visible.document.uri.toString() === uri.toString()
    );
    assert(firstVisible, 'first-open status editor is not visible');
    assert(
      firstVisible.visibleRanges.some((range) => range.start.line === 0),
      'first-open status body is not in the visible editor'
    );
    const firstShot = shot('r-vs-2-first-open.png');

    if (process.env.MAGIT_VSCODE_EDIT_ONLY === '1') {
      await require('./real-file-native').run({ extension, evidence, repo, cdp, waitFor, shot,
        requests, draftNotes });
      return;
    }
    if (process.env.MAGIT_VSCODE_HEAD_ONLY === '1') {
      await require('./head-operations').run({ extension, evidence, repo, cdp, waitFor, shot,
        requests, draftMessages, draftNotes });
      return;
    }

    // Isolate native selection mutations before unrelated widgets alter focus.
    if (process.env.MAGIT_VSCODE_A34_ONLY === '1') {
      const { store } = require(`${extension.extensionPath}/lib/store`);
      const git = (...args) => execFileSync('git', ['-C', repo, ...args]);
      const file = path.join(repo, 'tracked.txt');
      const prefix = 'head\ncontext\n';
      const unchanged = `${prefix}tail\n`;
      const changed = `${prefix}FIRST\nSECOND\nGUARD\ntail\n`;
      const cases = [];
      const screenshots = [];
      const paintedStatus = () => {
        const lines = cdp('Runtime.evaluate', {
          expression: `Array.from(document.querySelectorAll('.monaco-editor .view-lines .view-line'))
            .sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top)
            .map(line => line.textContent.replace(/\\u00a0/g, ' ').trim()).filter(Boolean)`,
          returnByValue: true,
        }).result.value;
        return JSON.stringify(lines) === JSON.stringify(editor.document.getText().split('\n')
          .map((line) => line.trim()).filter(Boolean));
      };
      const snapshot = () => ({
        anchor: [editor.selection.anchor.line, editor.selection.anchor.character],
        active: [editor.selection.active.line, editor.selection.active.character],
        start: [editor.selection.start.line, editor.selection.start.character],
        end: [editor.selection.end.line, editor.selection.end.character],
        visibleRanges: editor.visibleRanges.map((range) => [range.start.line, range.end.line]),
        text: editor.document.getText(), focus: observe(),
      });
      for (const deletion of [false, true]) {
        const base = deletion ? changed : unchanged;
        const work = deletion ? unchanged : changed;
        fs.writeFileSync(file, base);
        git('add', 'tracked.txt');
        git('-c', 'user.name=Test', '-c', 'user.email=test@example.com',
          'commit', '--allow-empty', '-qm', `A34 ${deletion ? 'deletion' : 'addition'}`);
        for (const unstage of [false, true]) {
          for (const nonzero of [false, true]) {
            for (const method of ['view/key', 'view/invoke']) {
              const reverse = (method === 'view/invoke') !== unstage;
              const name = `a34-${deletion ? 'delete' : 'add'}-${unstage ? 'unstage' : 'stage'}-${nonzero ? 'nonzero' : 'colzero'}-${method.slice(5)}`;
              fs.writeFileSync(file, base);
              git('add', 'tracked.txt');
              fs.writeFileSync(file, work);
              if (unstage) git('add', 'tracked.txt');
              editor.selection = new vscode.Selection(0, 0, 0, 0);
              await vscode.commands.executeCommand('magit.invoke', { command: 'common.refresh' });
              let hunk;
              await waitFor(async () => {
                const symbols = await store.client.sendRequest('textDocument/documentSymbol', {
                  textDocument: { uri: store.views.get(uri.authority).descriptor.uri },
                });
                fs.writeFileSync(`${evidence}/a34-symbols.json`, JSON.stringify(symbols, null, 2));
                const files = symbols.filter((symbol) => symbol.name ===
                  (unstage ? 'Staged changes' : 'Unstaged changes')).flatMap((symbol) => symbol.children || []);
                hunk = files.find((symbol) => symbol.name.includes('tracked.txt'))?.children?.[0];
                const first = hunk?.range.start.line + 3; // header + two known fixture context rows
                return hunk && ['FIRST', 'SECOND', 'GUARD'].every((line, offset) =>
                  editor.document.lineAt(first + offset).text === `    ${deletion ? '-' : '+'}${line}`);
              }, `${name} acknowledged structured fixture`);
              const first = hunk.range.start.line + 3;
              assert(hunk.range.end.line >= first + 2);
              // Include a context row and stop at the adjacent SECOND boundary.
              const start = [first - 1, 0];
              const end = [first + 1, nonzero ? 1 : 0];
              const anchor = reverse ? end : start;
              const active = reverse ? start : end;
              await vscode.window.showTextDocument(editor.document, { preview: false });
              await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
              editor.selection = new vscode.Selection(new vscode.Position(...anchor), new vscode.Position(...active));
              editor.revealRange(new vscode.Range(first - 1, 0, first + 2, 0), vscode.TextEditorRevealType.InCenter);
              await waitFor(() => observe().editorFocused && editor.visibleRanges.some((range) =>
                range.start.line <= first - 1 && range.end.line >= first + 2), `${name} visible selection`);
              assert(editor.selection instanceof vscode.Selection);
              const before = snapshot();
              assert.deepEqual(before.anchor, anchor);
              assert.deepEqual(before.active, active, 'active cursor is independent of ordered range');
              assert.deepEqual(before.start, start);
              assert.deepEqual(before.end, end);
              await waitFor(paintedStatus, `${name} painted pre-mutation status`);
              screenshots.push(shot(`${name}-before.png`));
              const offset = requests.length;
              const version = store.views.get(uri.authority).version;
              const command = unstage ? 'status.unstage' : 'status.stage';
              const key = unstage ? 'u' : 's';
              await vscode.commands.executeCommand(method === 'view/key' ? 'magit.key' : 'magit.invoke',
                method === 'view/key' ? { key } : { command });
              const sent = requests.slice(offset).filter((request) =>
                request.method === 'view/key' || request.method === 'view/invoke');
              assert.equal(sent.length, 1);
              assert.deepEqual(sent[0], { method, params: {
                viewId: uri.authority, version,
                ...(method === 'view/key' ? { key, editorMode: 'visual' } : { command }),
                cursor: { line: active[0], column: active[1] },
                selection: { startLine: start[0], endLine: first + (nonzero ? 1 : 0) },
              } });
              const selected = nonzero ? ['FIRST', 'SECOND'] : ['FIRST'];
              const retained = ['FIRST', 'SECOND', 'GUARD'].filter((line) => {
                const applied = unstage ? !selected.includes(line) : selected.includes(line);
                return deletion ? !applied : applied;
              });
              const expected = `${prefix}${retained.length ? `${retained.join('\n')}\n` : ''}tail\n`;
              assert.deepEqual(git('show', ':tracked.txt'), Buffer.from(expected), `${name} exact index with untouched neighbors`);
              assert.deepEqual(fs.readFileSync(file), Buffer.from(work), `${name} untouched worktree`);
              await waitFor(() => store.views.get(uri.authority).version > version &&
                editor.document.getText().includes('Staged changes (1)') &&
                editor.document.getText().includes('Unstaged changes (1)') &&
                editor.document.getText() === store.views.get(uri.authority).lines.join('\n'), `${name} visible resulting status`);
              const after = snapshot();
              assert(after.focus.editorFocused, 'result remains in native editor');
              await waitFor(paintedStatus, `${name} painted resulting status`);
              screenshots.push(shot(`${name}-after.png`));
              cases.push({ name, deletion, unstage, nonzero, reverse, hunk, before, request: sent[0], after,
                bytes: { head: git('show', 'HEAD:tracked.txt').toString('base64'),
                  index: git('show', ':tracked.txt').toString('base64'), worktree: fs.readFileSync(file).toString('base64'),
                  expectedIndex: Buffer.from(expected).toString('base64') } });
            }
          }
        }
      }
      fs.writeFileSync(`${evidence}/a34-evidence.json`, JSON.stringify({ gate: 'A34', cases, screenshots }, null, 2));
      console.log(`PASS native A34 selections (${cases.length} exact request/index/worktree cases)`);
      return;
    }

    // Native A33 uses real commit/dispatch metadata in the owned Vim fixture.
    if (process.env.MAGIT_VSCODE_A33_ONLY === '1') {
      const { store } = require(`${extension.extensionPath}/lib/store`);
      const views = require(`${extension.extensionPath}/lib/views`);
      const key = async (name) => {
        if (execFileSync('xdotool', ['getactivewindow']).toString().trim() !== windowId) {
          execFileSync('xdotool', ['windowactivate', '--sync', windowId]);
        }
        execFileSync('xdotool', ['key', '--clearmodifiers', name]);
      };
      const openMenu = async (command, kind) => {
        await vscode.window.showTextDocument(editor.document, { preview: false });
        await vscode.commands.executeCommand('magit.invoke', { command });
        await waitFor(() => [...store.views.values()].some((view) =>
          view.descriptor.kind === kind && view.quickPick && view.menu), kind);
        return [...store.views.values()].find((view) => view.descriptor.kind === kind && view.quickPick);
      };
      const activate = async (state, canonical, steps, name, outcome) => {
        await waitFor(() => quickInputState().visible, `${name} QuickPick`);
        const row = state.menu.groups.flatMap((group) => group.items).find((item) => item.key === canonical);
        assert(row, `${name} row missing from server menu`);
        assert.deepEqual(row.activationSteps, steps, `${name} activation metadata`);
        // Navigate the actual native list; typing would exercise routeTyped,
        // not acceptance. No synthetic rows or context keys are installed.
        const items = state.quickPick.items.filter((item) => item.key);
        const target = items.findIndex((item) => item.key === canonical);
        await waitFor(() => state.quickPick.activeItems[0]?.key, `${name} initial selection`);
        const initial = items.findIndex((item) => item.key === state.quickPick.activeItems[0].key);
        assert(initial >= 0 && target >= initial, 'expected initial native selection before target');
        for (let index = initial; index < target; index++) await key('Down');
        await waitFor(() => state.quickPick.activeItems[0]?.key === canonical, `${name} native selection`);
        shot(`a33-${name}-before.png`);
        const start = requests.length;
        await key('Return');
        await waitFor(outcome, `${name} visible outcome`);
        await state.pressQueue;
        const sent = requests.slice(start).filter((request) => request.method === 'view/key');
        assert.deepEqual(sent.map((request) => ({ viewId: request.params.viewId, key: request.params.key })),
          steps.map((step) => ({ viewId: state.id, key: step })), `${name} exact activation requests`);
        shot(`a33-${name}-after.png`);
        return { row, sent, mode: state.mode, rendered: state.lines, visible: quickInputState() };
      };
      const cases = [];
      const commit = await openMenu('open.commit', 'menu.commit');
      assert.equal(commit.menu.groups.flatMap((group) => group.items).find((item) => item.key === '-a').on, false);
      cases.push(await activate(commit, '-a', ['-', 'a'], 'flag', () =>
        commit.menu.groups.flatMap((group) => group.items).find((item) => item.key === '-a').on === true
        && commit.quickPick.items.some((item) => item.key === '-a' && item.description.includes('on'))));
      await key('Escape');
      await waitFor(() => !store.views.has(commit.id), 'commit menu closed');
      const dispatch = await openMenu('open.dispatch', 'menu.dispatch');
      cases.push(await activate(dispatch, 'ctrl+t', ['ctrl+t'], 'modifier', () => dispatch.mode === 'text'));
      assert.equal(dispatch.quickPick.title, 'Dispatch');
      await key('Escape');
      await waitFor(() => !store.views.has(dispatch.id), 'dispatch menu closed');
      const spaced = await openMenu('open.dispatch', 'menu.dispatch');
      cases.push(await activate(spaced, 'g l', ['g', 'l'], 'spaced', () =>
        [...store.views.values()].some((view) => view.descriptor.kind === 'menu.log' && view.quickPick?.title === 'Log')
        && quickInputState().text?.includes('Log')));
      fs.writeFileSync(`${evidence}/a33-evidence.json`, JSON.stringify({
        gate: 'A33', profile: 'vim', cases, screenshots: ['a33-flag-before.png', 'a33-flag-after.png',
          'a33-modifier-before.png', 'a33-modifier-after.png', 'a33-spaced-before.png', 'a33-spaced-after.png'],
        scope: 'Native QuickPick acceptance of server commit/dispatch rows. Named, uppercase and Plus remain automated integration coverage.',
      }, null, 2));
      console.log('PASS native A33 QuickPick activation metadata');
      return;
    }

    // Run A29 before any unrelated widget/theme tests can leave foreign focus.
    if (process.env.MAGIT_VSCODE_A29_ONLY === '1') {
      const { store } = require(`${extension.extensionPath}/lib/store`);
      const views = require(`${extension.extensionPath}/lib/views`);
      const { deriveContext } = require(`${extension.extensionPath}/lib/context`);
      const key = async (name) => {
        if (execFileSync('xdotool', ['getactivewindow']).toString().trim() !== windowId) {
          execFileSync('xdotool', ['windowactivate', '--sync', windowId]);
        }
        execFileSync('xdotool', ['key', '--clearmodifiers', name]);
      };
      const selection = (target) => ({ anchor: [target.selection.anchor.line,
        target.selection.anchor.character], active: [target.selection.active.line,
        target.selection.active.character] });
      const focus = async (target) => {
        await vscode.window.showTextDocument(target.document, { preserveFocus: false });
        await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
        if (quickInputState().visible) await key('Escape');
        await waitFor(() => !quickInputState().visible, 'no foreign quick input');
        await waitFor(() => observe().editorFocused, 'native editor focus');
        assert.equal(vscode.window.activeTextEditor.document.uri.toString(), target.document.uri.toString());
      };
      await vscode.commands.executeCommand('magit.invoke', { command: 'common.show-all' });
      await waitFor(() => editor.document.getText().includes('+replacement'), 'expanded A29 status');
      await focus(editor);
      await vscode.commands.executeCommand('workbench.action.toggleKeybindingsLog');
      await focus(editor);
      const state = store.views.get(uri.authority);
      const beforeText = editor.document.getText();
      const versionBefore = renderEvents.filter((event) => event.viewId === state.id).at(-1).version;
      const preKey = { dom: observe(), quickInput: quickInputState(), uri: uri.toString(),
        mode: state.mode, context: deriveContext(state.descriptor, state.mode) };
      shot('a29-before.png');
      const start = requests.length;
      startKeyProbe();
      await key('ctrl+t');
      fs.writeFileSync(`${evidence}/a29-pre-toggle.json`, JSON.stringify({ preKey,
        keyEvents: cdp('Runtime.evaluate', { expression: 'window.__magitKeyEvents', returnByValue: true }).result.value,
        after: observe(), quickInput: quickInputState(),
        log: cdp('Runtime.evaluate', { expression: `document.querySelector('.output-view')?.innerText`, returnByValue: true }).result.value,
      }, null, 2));
      await waitFor(() => state.mode === 'text', 'physical text-mode toggle');
      const entryKeys = stopKeyProbe();
      const entryRender = renderEvents.filter((event) => event.viewId === state.id).at(-1);
      assert.equal(entryRender.version, versionBefore, 'entry must be same-version metadata');
      assert.equal(entryRender.text, undefined);
      assert.equal(entryRender.edits, undefined);
      const quietStart = requests.length;
      editor.selection = new vscode.Selection(0, 0, 0, 0);
      await key('Down');
      await waitFor(() => editor.selection.active.line === 1, 'native arrow moves cursor');
      const arrowSelection = selection(editor);
      await key('ctrl+f');
      await waitFor(() => observe().aria === 'Find', 'native Find focus');
      execFileSync('xdotool', ['type', '--clearmodifiers', '--delay', '25', 'replacement']);
      await waitFor(() => observe().value === 'replacement', 'native search query');
      await key('Return');
      await waitFor(() => editor.document.getText(editor.selection) === 'replacement', 'native search result');
      const searchSelection = selection(editor);
      await key('Escape');
      await waitFor(() => !observe().findVisible, 'Find dismissed before editor copy');
      await focus(editor);
      await key('Home');
      await key('shift+End');
      await waitFor(() => editor.document.getText(editor.selection) === '+replacement', 'native editor line selection');
      await vscode.env.clipboard.writeText('A29 clipboard sentinel');
      await key('ctrl+c');
      await waitFor(async () => await vscode.env.clipboard.readText() === '+replacement', 'native editor copy');
      const copiedText = await vscode.env.clipboard.readText();
      const selected = selection(editor);
      shot('a29-during.png');
      await key('ctrl+alt+bracketright');
      await key('g');
      await settle(300);
      assert.equal(requests.length, quietStart, 'native browsing/ordinary/mirrored keys sent view traffic');
      assert.equal(editor.document.getText(), beforeText);
      assert.deepEqual(selection(editor), selected);
      fs.writeFileSync(path.join(repo, 'tracked.txt'), 'a29 queued change\n');
      await vscode.commands.executeCommand('magit.invoke', { command: 'common.refresh' });
      await settle(500);
      assert.equal(editor.document.getText(), beforeText, 'queued refresh changed rendered text');
      assert.deepEqual(selection(editor), selected, 'queued refresh moved selection');
      shot('a29-queued.png');
      await key('ctrl+t');
      await waitFor(() => state.mode === 'interactive', 'physical return toggle');
      await waitFor(() => editor.document.getText().includes('+a29 queued change'), 'queued render applied');
      assert.equal(editor.document.getText(), renderedLines.get(state.id).join('\n'));
      shot('a29-after.png');

      // Only menu views currently advertise capture=true (log is false too).
      // Open the real menu's provider URI in the native editor, without a
      // QuickPick or synthetic context, to exercise its actual capture binding.
      const descriptor = await store.client.sendRequest('view/open', {
        kind: 'menu.dispatch', args: { repo_path: repo }, viewport: { cols: 100, rows: 30 },
      });
      const logState = views.ensureView(descriptor);
      await logState.firstRender;
      const logEditor = await vscode.window.showTextDocument(
        await vscode.workspace.openTextDocument(logState.uri), { preserveFocus: false });
      assert.equal(descriptor.capture, true, 'menu must exercise capture-enabled input');
      await focus(logEditor);
      await key('ctrl+t');
      await waitFor(() => logState.mode === 'text', 'capture-enabled view enters text mode');
      const logText = logEditor.document.getText();
      const logStart = requests.length;
      await key('x');
      await key('g');
      await key('ctrl+alt+bracketright');
      await settle(300);
      assert.equal(requests.length, logStart, 'capture-enabled text view sent keys');
      assert.equal(logEditor.document.getText(), logText, 'virtual document must stay read-only');
      shot('a29-capture.png');
      await key('ctrl+t');
      await waitFor(() => logState.mode === 'interactive', 'capture-enabled view returns');
      fs.writeFileSync(`${evidence}/a29-evidence.json`, JSON.stringify({
        gate: 'A29', uri: uri.toString(), preKey, entryKeys, entryRender, versionBefore,
        arrowSelection, searchSelection, copiedText, selected, initialText: beforeText,
        queuedSelection: selected, finalText: editor.document.getText(),
        requests: requests.slice(start), captureDescriptor: descriptor,
        captureSuppressedRequests: requests.slice(logStart, -1),
        screenshots: ['a29-before.png', 'a29-during.png', 'a29-queued.png', 'a29-after.png', 'a29-capture.png'],
      }, null, 2));
      console.log('PASS native A29 text-mode navigation/search/editor copy, key suppression and queued refresh');
      return;
    }

    const verifyQuitReopen = async () => {
      const { store } = require(`${extension.extensionPath}/lib/store`);
      const views = require(`${extension.extensionPath}/lib/views`);
      const otherRepo = path.join(evidence, 'other-repo');
      execFileSync('git', ['clone', '-q', repo, otherRepo]);
      const descriptor = await store.client.sendRequest('view/open', {
        kind: 'status', args: { repo_path: otherRepo }, viewport: { cols: 100, rows: 30 },
      });
      const other = views.ensureView(descriptor);
      other.rootId = other.id;
      other.repo = otherRepo;
      await views.showView(descriptor);
      const root = views.stateForUri(uri);
      await views.focusView(root.id);
      await vscode.commands.executeCommand('magit.invoke', { command: 'open.commit' });
      await waitFor(() => quickInputState().visible, 'nested menu visible');
      await vscode.commands.executeCommand('magit.invoke', { command: 'common.quit' });
      await waitFor(() => !quickInputState().visible, 'nested menu popped');
      assert(store.views.has(root.id), 'nested quit removed root');
      await views.focusView(root.id);
      await vscode.commands.executeCommand('magit.invoke', { command: 'open.commit' });
      await waitFor(() => quickInputState().visible
        && /Commit/i.test(quickInputState().text || ''), 'native commit QuickPick');
      shot('a06-menu-before-quit.png');
      const removed = [...store.views.values()].filter((state) => state.rootId === root.id);
      const beforeClose = requests.filter((request) => request.method === 'view/close').length;
      // Quitting the menu only pops it; target the root with that menu open.
      await require(`${extension.extensionPath}/lib/requests`).invoke(root.id, 'common.quit', undefined,
        { cursor: { line: 0, column: 0 } });
      await waitFor(() => removed.every((state) => !statusTabs().some((tab) =>
        tab.input.uri.toString() === state.uri.toString())), 'root-owned tabs closed');
      await waitFor(() => !quickInputState().visible, 'QuickPick dismissed');
      assert.equal(requests.filter((request) => request.method === 'view/close').length, beforeClose,
        'server quit did not recursively request view/close');
      for (const state of removed) {
        assert(!store.views.has(state.id), 'quit retained view');
        assert(!store.uriToView.has(state.uri.toString()), 'quit retained URI');
        assert(!store.pendingRenders.has(state.id), 'quit retained pending render');
        assert(!state.quickPick, 'quit retained QuickPick');
      }
      assert(store.views.has(other.id), 'quit removed unrelated root');
      assert(statusTabs().some((tab) => tab.input.uri.toString() === other.uri.toString()),
        'quit closed unrelated tab');
      await vscode.window.showTextDocument(vscode.Uri.file(path.join(repo, 'tracked.txt')));
      await waitFor(() => store.activeViewId === undefined, 'inactive view context');
      const afterQuitCount = requests.length;
      await vscode.commands.executeCommand('magit.key', { key: 'g' });
      await settle(300);
      assert.equal(requests.length, afterQuitCount, 'inactive contexts dispatched a Magit key');
      shot('a06-quit.png');
      await vscode.commands.executeCommand('magit.open');
      await waitFor(() => statusTabs().length === 2, 'status reopened alongside other root');
      const reopened = vscode.window.activeTextEditor;
      assert(reopened && reopened.document.uri.scheme === 'magit');
      await waitFor(() => reopened.document.getText().includes('tracked.txt'),
        'reopened status render');
      shot('a06-reopened.png');
      const refreshCount = requests.length;
      execFileSync('xdotool', ['key', '--window', windowId, '--clearmodifiers', 'g']);
      await waitFor(() => requests.slice(refreshCount).some((request) =>
        request.method === 'view/invoke' && request.params.command === 'common.refresh'),
      'reopened status responds to physical refresh');
      fs.writeFileSync(`${evidence}/a06-evidence.json`, JSON.stringify({
        menuBeforeQuit: 'a06-menu-before-quit.png',
        quit: 'a06-quit.png',
        reopened: 'a06-reopened.png',
        affectedTabsAfterQuit: 0,
        preservedRoot: other.id,
        removedViews: removed.map((state) => state.id),
        reopenedRoot: views.activeState().rootId,
        reopenedRefresh: requests.slice(refreshCount),
        quickPickHiddenAfterQuit: true,
        noRecursiveViewClose: true,
        keyDispatchedAfterQuit: false,
      }, null, 2));
      console.log('PASS native A06 root quit closes menu/tabs and reopens status');
    };
    if (process.env.MAGIT_VSCODE_A06_ONLY === '1') {
      await verifyQuitReopen();
      return;
    }

    // Repeat and concurrent opens must focus the one status tab, not create a
    // second view or a temporary filename document.
    const viewOpensBeforeReopen = requests.filter(
      (request) => request.method === 'view/open'
    ).length;
    await vscode.commands.executeCommand('magit.open');
    await Promise.all([
      vscode.commands.executeCommand('magit.open'),
      vscode.commands.executeCommand('magit.open'),
    ]);
    assert.equal(
      requests.filter((request) => request.method === 'view/open').length,
      viewOpensBeforeReopen,
      'repeat/concurrent opens requested another status view'
    );
    assert.equal(vscode.window.activeTextEditor.document.uri.toString(), uri.toString());
    assert.equal(statusTabs().length, 1, 'repeat/concurrent opens created another status tab');
    assert.equal(statusTabs()[0].label, openedViews[0].title, 'reopen tab label is not the view title');
    assert.equal(editor.document.getText(), first, 'reopen changed the server-owned status text');
    assert.equal(fs.existsSync(statusPath), false, 'reopen created a backing status file');
    assert.deepEqual(workspaceFiles(repo), filesBefore, 'reopen created a workspace file');
    await settle(300);
    const reopenVisible = vscode.window.visibleTextEditors.find(
      (visible) => visible.document.uri.toString() === uri.toString()
    );
    assert(reopenVisible, 'reopened status editor is not visible');
    assert(
      reopenVisible.visibleRanges.some((range) => range.start.line === 0),
      'reopened status body is not in the visible editor'
    );
    const reopenShot = shot('r-vs-2-reopen.png');
    const reopenTextUnchanged = editor.document.getText() === first;

    await vscode.commands.executeCommand('magit.invoke', { command: 'common.show-all', args: {} });
    await waitFor(() => editor.document.getText().includes('+replacement'), 'expanded status diff');

    // Use the server's semantic-token ranges instead of parsing display text.
    const legend = await vscode.commands.executeCommand(
      'vscode.provideDocumentSemanticTokensLegend', uri
    );
    let tokens;
    await waitFor(async () => {
      tokens = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokens', uri);
      return tokens?.data.length > 0;
    }, 'semantic tokens');
    const tokenEntries = [];
    let tokenLine = 0;
    let tokenStart = 0;
    for (let index = 0; index < tokens.data.length; index += 5) {
      tokenLine += tokens.data[index];
      tokenStart = tokens.data[index] === 0
        ? tokenStart + tokens.data[index + 1]
        : tokens.data[index + 1];
      tokenEntries.push({
        line: tokenLine,
        start: tokenStart,
        length: tokens.data[index + 2],
        type: legend.tokenTypes[tokens.data[index + 3]],
      });
    }
    const types = new Set(tokenEntries.map((entry) => entry.type));
    for (const type of ['magit-section', 'magit-diff-added', 'magit-diff-removed']) {
      assert(types.has(type), type);
    }
    // R-VS-4: the host must request the server's semantic tokens and the
    // active theme must actually paint section headings and both diff
    // directions. Spans are located from the decoded server token ranges and
    // the server-owned document lines, never by parsing display text.
    assert(semanticRequests.length > 0, 'the extension host never requested semantic tokens');
    assert(
      semanticRequests.some(
        (request) => request.params.textDocument.uri === openedViews[0].uri
      ),
      'semantic-token requests did not use the server view URI'
    );
    const documentLines = () => editor.document.getText().split('\n');
    const themeTokens = tokenEntries.filter(
      (entry) => ['magit-section', 'magit-diff-added', 'magit-diff-removed'].includes(entry.type)
    );
    const styleEvidence = (entries) => {
      const lines = documentLines();
      const wanted = entries.map((entry) => ({
        type: entry.type,
        line: entry.line,
        start: entry.start,
        length: entry.length,
        lineText: lines[entry.line] || '',
        text: (lines[entry.line] || '').slice(entry.start, entry.start + entry.length),
      }));
      const measured = cdp('Runtime.evaluate', {
        expression: `(() => {
          const wanted = ${JSON.stringify(wanted)};
          const normalize = (text) => text.replace(/\\u00a0/g, ' ');
          const viewLines = [...document.querySelectorAll('.view-line')];
          const lineTexts = viewLines.map((element) => normalize(element.textContent));
          const editorElement = document.querySelector('.monaco-editor');
          const workbench = document.querySelector('.monaco-workbench');
          const editorStyle = editorElement && getComputedStyle(editorElement);
          const tokens = wanted.map((token) => {
            const index = lineTexts.indexOf(token.lineText);
            if (index < 0) return { ...token, found: false };
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
            const cell = cells[Math.min(token.start, Math.max(0, cells.length - 1))];
            const style = cell && getComputedStyle(cell.element);
            return { ...token, found: true,
              html: lineElement.outerHTML.slice(0, 400),
              className: cell && cell.element.className,
              color: style && style.color,
              backgroundColor: style && style.backgroundColor,
              fontWeight: style && style.fontWeight,
              fontStyle: style && style.fontStyle,
              textDecoration: style && style.textDecorationLine };
          });
          return {
            themeClass: workbench && workbench.className,
            background: editorStyle && editorStyle.backgroundColor,
            defaultColor: editorStyle && editorStyle.color,
            tokens,
          };
        })()`,
        returnByValue: true,
      }).result.value;
      // The document is still exactly the server-owned render.
      assert.equal(
        editor.document.getText(),
        (renderedLines.get(uri.authority) || []).join('\n'),
        'theme measurement changed the server-owned text'
      );
      return measured;
    };
    const themeSetting = () => vscode.workspace.getConfiguration('workbench').get('colorTheme');
    const isDarkWorkbench = (measurement) => /(^|\s)vs-dark(\s|$)/.test(
      measurement.themeClass || ''
    );
    // A theme change repaints default colours first and semantic tokens
    // asynchronously. Wait for the measured spans themselves, not just the
    // workbench class or a fixed pause.
    const isPainted = (measurement) => measurement.tokens.every(
      (token) => token.found && token.color !== measurement.defaultColor
    );
    const waitForStyles = async (predicate, label) => {
      let last;
      await waitFor(() => {
        last = styleEvidence(themeTokens);
        return predicate(last);
      }, label, 20000);
      await settle(200);
      return styleEvidence(themeTokens);
    };
    const initialTheme = themeSetting();
    assert(initialTheme, 'no active workbench theme');
    const darkStyles = styleEvidence(themeTokens);
    assert(isDarkWorkbench(darkStyles), 'the isolated host did not start on a dark theme');
    assert(isPainted(darkStyles), 'the dark theme did not paint the status tokens');
    const darkText = editor.document.getText();
    const darkThemeShot = shot('r-vs-4-theme-dark.png');
    // Record the first stage before switching, so a later failure still leaves
    // the measured dark-theme evidence inspectable.
    const writeThemeEvidence = (light, restored) => fs.writeFileSync(
      `${evidence}/r-vs-4-evidence.json`,
      JSON.stringify({
        gate: 'R-VS-4',
        uri: uri.toString(),
        serverViewUri: openedViews[0].uri,
        legend,
        contributions: {
          semanticTokenTypes: extension.packageJSON.contributes.semanticTokenTypes,
          semanticTokenScopes: extension.packageJSON.contributes.semanticTokenScopes,
          configurationDefaults: extension.packageJSON.contributes.configurationDefaults,
        },
        semanticTokenRequests: semanticRequests,
        themes: {
          initial: initialTheme,
          switched: 'Light Modern',
          restored: themeSetting(),
        },
        dark: darkStyles,
        light,
        restored,
        text: {
          dark: darkText,
          light: light && light.text,
          restored: restored && restored.text,
          identical: Boolean(light && restored)
            && light.text === darkText && restored.text === darkText,
          serverRender: (renderedLines.get(uri.authority) || []).join('\n'),
        },
        screenshots: [darkThemeShot, light && light.shot, restored && restored.shot]
          .filter(Boolean),
      }, null, 2)
    );
    writeThemeEvidence(undefined, undefined);
    await vscode.workspace.getConfiguration('workbench').update(
      'colorTheme', 'Light Modern', vscode.ConfigurationTarget.Workspace
    );
    const lightStyles = await waitForStyles(
      (measurement) => !isDarkWorkbench(measurement) && isPainted(measurement),
      'bundled light theme painting the status tokens'
    );
    const lightText = editor.document.getText();
    const lightThemeShot = shot('r-vs-4-theme-light.png');
    writeThemeEvidence({ ...lightStyles, text: lightText, shot: lightThemeShot }, undefined);
    await vscode.workspace.getConfiguration('workbench').update(
      'colorTheme', initialTheme, vscode.ConfigurationTarget.Workspace
    );
    const restoredStyles = await waitForStyles(
      (measurement) => isDarkWorkbench(measurement) && isPainted(measurement),
      'bundled dark theme painting the status tokens again'
    );
    const restoredText = editor.document.getText();
    const restoredThemeShot = shot('r-vs-4-theme-dark-restored.png');
    assert.equal(themeSetting(), initialTheme, 'theme was not restored');
    assert.equal(lightText, darkText, 'switching theme changed the server text');
    assert.equal(restoredText, darkText, 'restoring the theme changed the server text');
    const colorOf = (measurement, type) => measurement.tokens.find(
      (token) => token.type === type
    )?.color;
    for (const [label, measurement] of [
      ['dark', darkStyles], ['light', lightStyles], ['dark-restored', restoredStyles],
    ]) {
      assert(
        measurement.tokens.every((token) => token.found),
        `${label}: token span missing from the rendered document`
      );
      for (const token of measurement.tokens) {
        assert.notEqual(
          token.color,
          measurement.defaultColor,
          `${label}: ${token.type} is painted with the default foreground`
        );
      }
    }
    for (const type of ['magit-section', 'magit-diff-added', 'magit-diff-removed']) {
      assert.notEqual(
        colorOf(darkStyles, type),
        colorOf(lightStyles, type),
        `${type} did not follow the active theme`
      );
      assert.equal(
        colorOf(restoredStyles, type),
        colorOf(darkStyles, type),
        `${type} did not return to its dark-theme colour`
      );
    }
    writeThemeEvidence(
      { ...lightStyles, text: lightText, shot: lightThemeShot },
      { ...restoredStyles, text: restoredText, shot: restoredThemeShot }
    );

    const fileLine = tokenEntries.find((entry) => entry.type.startsWith('magit-file-'))?.line;
    assert.notEqual(fileLine, undefined, 'no file token to focus');
    const sectionLine = tokenEntries.find(
      (entry) => entry.type === 'magit-section' && entry.line > 2
    )?.line;
    assert.notEqual(sectionLine, undefined, 'no section token to fold');
    const selectLine = (line) => {
      editor.selection = new vscode.Selection(line, 0, line, 0);
      editor.revealRange(new vscode.Range(line, 0, line, 0));
    };

    // Target only this isolated host window, never a user's existing editor.
    const key = async (name) => {
      // Re-activating an already active Chromium window can move focus away
      // from its find textarea back into the editor's native edit context.
      let active;
      try {
        active = execFileSync('xdotool', ['getactivewindow']).toString().trim();
      } catch {
        // The window manager may momentarily report no active window at all;
        // that is not proof the isolated window is active, so activate below.
        active = '';
      }
      if (active !== windowId) {
        execFileSync('xdotool', ['windowactivate', '--sync', windowId]);
      }
      assert.equal(execFileSync('xdotool', ['getactivewindow']).toString().trim(), windowId);
      execFileSync('xdotool', ['key', '--clearmodifiers', name]);
    };
    // XTEST typing needs the real window focused; `xdotool type --window`
    // uses synthetic events that Chromium ignores.
    const type = (text) => execFileSync('xdotool', [
      'type', '--clearmodifiers', '--delay', '25', text,
    ]);
    const focusStatus = async () => {
      await vscode.window.showTextDocument(editor.document, { preview: false, preserveFocus: false });
      await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
      assert.equal(vscode.window.activeTextEditor.document.uri.toString(), uri.toString());
    };
    await focusStatus();
    shot('00-status-expanded.png');

    // 1. A foreign QuickPick over the focused status must keep its own input.
    let pickValue = '';
    let picked;
    const pick = vscode.window.createQuickPick();
    pick.title = 'Foreign QuickPick';
    pick.items = [{ label: 'alpha' }, { label: 'beta g' }];
    pick.onDidChangeValue((value) => { pickValue = value; });
    pick.onDidAccept(() => {
      picked = pick.selectedItems[0]?.label;
      pick.hide();
    });
    const beforePick = magitTraffic();
    pick.show();
    await settle(300);
    await key('ctrl+alt+bracketright');
    await type('beta g');
    await waitFor(() => pickValue === 'beta g', 'foreign QuickPick typed value');
    const pickShot = shot('01-foreign-quickpick.png');
    await key('Return');
    await waitFor(() => picked === 'beta g', 'foreign QuickPick Enter accepting beta g');
    assert.equal(magitTraffic(), beforePick, 'foreign QuickPick typing dispatched Magit traffic');
    pick.dispose();
    matrix.push({
      widget: 'foreign QuickPick', input: 'mirrored ctrl+alt+], beta g + Enter', result: 'typed beta g and accepted beta g',
      magitRequests: 0, screenshot: pickShot,
    });
    interactions.push('foreign QuickPick typed beta g and accepted beta g with no Magit dispatch');

    // 2. A foreign InputBox over the focused status must keep its own input.
    const input = vscode.window.createInputBox();
    input.title = 'Foreign InputBox';
    input.placeholder = 'type here';
    let inputValue = '';
    let inputAccepted;
    input.onDidChangeValue((value) => { inputValue = value; });
    input.onDidAccept(() => {
      inputAccepted = input.value;
      input.hide();
    });
    const beforeInput = magitTraffic();
    input.show();
    await settle(300);
    await key('ctrl+alt+bracketright');
    await type('hello foreign');
    await waitFor(() => inputValue === 'hello foreign', 'foreign InputBox typed value');
    const inputShot = shot('02-foreign-inputbox.png');
    await key('Return');
    await waitFor(() => inputAccepted === 'hello foreign', 'foreign InputBox Enter accepting text');
    assert.equal(magitTraffic(), beforeInput, 'foreign InputBox typing dispatched Magit traffic');
    input.dispose();
    matrix.push({
      widget: 'foreign InputBox', input: 'mirrored ctrl+alt+], hello foreign + Enter',
      result: 'typed hello foreign and accepted it', magitRequests: 0, screenshot: inputShot,
    });
    interactions.push('foreign InputBox typed and accepted its text with no Magit dispatch');

    // 3. The find widget over the focused status must keep Ctrl+A/Ctrl+C/Enter.
    await focusStatus();
    editor.selection = new vscode.Selection(0, 0, 0, 0);
    await vscode.env.clipboard.writeText('');
    const beforeFind = magitTraffic();
    // Set up the widget through its public command, then prove keyboard input
    // using physical XTEST events. Opening it is not binding evidence.
    // `actions.find` occasionally leaves focus in the editor's native edit
    // context; re-open the widget once, then still require its actual focus.
    for (let attempt = 0; attempt < 2; attempt += 1) {
      await vscode.commands.executeCommand('actions.find');
      try {
        await waitFor(() => observe().aria === 'Find', 'focused Find textarea', 4000);
        break;
      } catch (error) {
        if (attempt === 1) throw error;
        await focusStatus();
      }
    }
    await key('ctrl+alt+bracketright');
    await key('ctrl+a');
    await type('g');
    await waitFor(() => observe().value === 'g', 'find input retaining canonical g');
    await key('ctrl+a');
    await type('tracked');
    await waitFor(() => observe().value === 'tracked', 'find input containing tracked');
    const findShot = shot('03-find-widget.png');
    // If the query reached the find input, Enter selects the match in this
    // read-only status document. A swallowed query would leave it unchanged.
    await key('Return');
    await waitFor(
      () => !editor.selection.isEmpty && editor.document.getText(editor.selection).includes('tracked'),
      'find widget typing a query that selects the match'
    );
    const findText = editor.document.getText(editor.selection);
    // Ctrl+A/Ctrl+C must act on the find input, not cancel the Magit view.
    await key('ctrl+a');
    await key('ctrl+c');
    let copied = '';
    await vscode.env.clipboard.readText().then((text) => { copied = text; });
    assert.equal(copied, 'tracked', 'find input copy did not retain its text');
    await key('Escape');
    await waitFor(
      () => vscode.window.activeTextEditor?.document.uri.toString() === uri.toString(),
      'status focus after find'
    );
    assert.equal(magitTraffic(), beforeFind, 'find widget keyboard input dispatched Magit traffic');
    matrix.push({
      widget: 'find widget', setup: 'actions.find (not physical opening-key evidence)',
      input: 'mirrored ctrl+alt+], g, ctrl+a, tracked, Enter, ctrl+a, ctrl+c',
      result: `find selected "${findText}" without Magit dispatch (clipboard: ${JSON.stringify(copied)})`,
      magitRequests: 0, screenshot: findShot,
    });
    interactions.push('find widget received the typed query and Enter selected the match without Magit dispatch');
    for (const widget of ['rename input', 'suggest widget']) {
      matrix.push({ widget, result: 'not exercised',
        reason: 'Read-only magit virtual document has no rename/completion provider; no fake context keys or synthetic provider installed. Unit when-clause exclusion only, not actual-client keyboard evidence.' });
    }

    // 4. Returning to status: Enter is delivered to the server again.
    await focusStatus();
    selectLine(fileLine);
    const beforeEnter = magitTraffic();
    await key('Return');
    await waitFor(() => magitTraffic() > beforeEnter, 'status Enter request');
    assert(invoked('common.visit'), 'status Enter did not dispatch common.visit');
    await waitFor(() => vscode.window.activeTextEditor?.document.uri.scheme === 'file',
      'Enter opens tracked file');
    assert.equal(vscode.window.activeTextEditor.document.uri.scheme, 'file');
    assert.equal(vscode.window.activeTextEditor.document.uri.fsPath, path.join(repo, 'tracked.txt'));
    const enterShot = shot('04-status-enter.png');
    matrix.push({
      widget: 'status editor', input: 'Enter', result: 'dispatched common.visit to the server',
      magitRequests: magitTraffic() - beforeEnter, screenshot: enterShot,
    });
    interactions.push('status Enter dispatched common.visit after foreign widgets');
    const togglesBefore = requests.filter(
      (request) => request.method === 'view/invoke'
        && request.params.command === 'common.toggle-section'
    ).length;

    // 5. Returning to status: Tab still folds through the server tree.
    await focusStatus();
    selectLine(sectionLine);
    const unfolded = editor.document.getText();
    await key('Tab');
    await waitFor(() => editor.document.getText() !== unfolded, 'status Tab folding');
    const tabFoldShot = shot('r-vs-3-fold.png');
    await key('Tab');
    await waitFor(() => editor.document.getText() === unfolded, 'status Tab unfolding');
    const toggleCount = requests.filter(
      (request) => request.method === 'view/invoke'
        && request.params.command === 'common.toggle-section'
    ).length;
    assert.equal(toggleCount - togglesBefore, 2, 'status Tab did not dispatch two toggles');
    const foldShot = shot('05-status-fold.png');
    matrix.push({
      widget: 'status editor', input: 'Tab twice',
      result: 'folded and unfolded the server section',
      magitRequests: 2, screenshot: foldShot,
    });
    interactions.push('status Tab folded/unfolded the server section after foreign widgets');
    const mirrorBefore = requests.length;
    await key('ctrl+alt+bracketright');
    await waitFor(() => editor.document.getText() !== unfolded, 'mirrored fold shortcut folding');
    await key('ctrl+alt+bracketright');
    await waitFor(() => editor.document.getText() === unfolded, 'mirrored fold shortcut unfolding');
    assert.equal(requests.slice(mirrorBefore).filter((request) =>
      request.method === 'view/invoke' && request.params.command === 'common.toggle-section').length, 2);
    matrix.push({ widget: 'status editor', input: 'mirrored ctrl+alt+] twice',
      result: 'folded and unfolded server section', magitRequests: requests.length - mirrorBefore,
      screenshot: shot('05b-mirrored-fold.png') });

    // 6. Returning to status: the canonical refresh binding re-reads Git and
    // must keep the same server-owned text and URI without a backing file.
    fs.writeFileSync(`${vscode.workspace.workspaceFolders[0].uri.fsPath}/tracked.txt`, 'after-return\n');
    const beforeRefresh = editor.document.getText();
    const refreshBefore = requests.length;
    await key('g');
    await waitFor(
      () => editor.document.getText().includes('+after-return'),
      'status g refresh',
      15000
    );
    const refreshed = editor.document.getText();
    const refreshedRender = (renderedLines.get(uri.authority) || []).join('\n');
    assert.notEqual(refreshed, beforeRefresh);
    assert(invoked('common.refresh'), 'status refresh did not dispatch common.refresh');
    assert.equal(refreshed, refreshedRender, 'refreshed text does not match the server render');
    assert.equal(editor.document.uri.toString(), uri.toString(), 'refresh changed the status view URI');
    assert.equal(statusTabs().length, 1, 'refresh created another status tab');
    assert.equal(statusTabs()[0].label, openedViews[0].title, 'refresh changed the tab label');
    assert.equal(fs.existsSync(statusPath), false, 'refresh created a backing status file');
    assert.deepEqual(workspaceFiles(repo), filesBefore, 'refresh created a workspace file');
    await settle(300);
    const refreshVisible = vscode.window.visibleTextEditors.find(
      (visible) => visible.document.uri.toString() === uri.toString()
    );
    assert(refreshVisible, 'refreshed status editor is not visible');
    const afterReturnLine = refreshed.split('\n')
      .findIndex((line) => line.includes('+after-return'));
    assert(afterReturnLine >= 0, 'refreshed text is missing the server-added line');
    assert(
      refreshVisible.visibleRanges.some(
        (range) => afterReturnLine >= range.start.line && afterReturnLine <= range.end.line
      ),
      'refreshed server line is not in the visible editor'
    );
    const refreshShot = shot('06-status-refresh.png');
    const rvs2RefreshShot = shot('r-vs-2-refresh.png');
    matrix.push({
      widget: 'status editor', input: 'g',
      result: 're-read the worktree diff from the server',
      magitRequests: requests.length - refreshBefore, screenshot: refreshShot,
    });
    interactions.push('status g refreshed the worktree diff after foreign widgets');
    fs.writeFileSync(`${evidence}/r-vs-2-evidence.json`, JSON.stringify({
      gate: 'R-VS-2',
      evidenceDirectory: evidence,
      repo,
      descriptor: openedViews[0],
      uri: uri.toString(),
      statusTab: { count: statusTabs().length, label: statusTabs()[0].label },
      workspaceFiles: { before: filesBefore, afterRefresh: workspaceFiles(repo) },
      viewOpenRequests: requests.filter((request) => request.method === 'view/open').length,
      firstOpen: {
        screenshot: firstShot,
        textMatchesServerRender: first === firstRender,
        lineCount: first.split('\n').length,
        visibleEditor: true,
        bodyLineVisible: true,
      },
      reopen: {
        screenshot: reopenShot,
        viewOpenRequestsBefore: viewOpensBeforeReopen,
        viewOpenRequestsAfter: requests.filter((request) => request.method === 'view/open').length,
        statusTabCount: statusTabs().length,
        statusTabLabel: statusTabs()[0].label,
        textUnchanged: reopenTextUnchanged,
        visibleEditor: true,
        bodyLineVisible: true,
      },
      refresh: {
        screenshot: rvs2RefreshShot,
        requests: requests.slice(refreshBefore).map((request) => ({
          method: request.method,
          command: request.params.command,
          key: request.params.key,
        })),
        textMatchesServerRender: refreshed === refreshedRender,
        visibleEditor: true,
        serverLineVisible: true,
        backingFileExists: fs.existsSync(statusPath),
      },
      renderEvents: renderEvents.map((event) => ({
        viewId: event.viewId,
        version: event.version,
        baseVersion: event.baseVersion,
        text: Array.isArray(event.text),
        edits: Array.isArray(event.edits),
      })),
    }, null, 2));

    // 7. R-VS-3: one generic dispatcher carries arbitrary canonical server
    // command IDs and opaque arguments; there is no VSCode command per action.
    const registeredCommands = await vscode.commands.getCommands(true);
    const registeredMagitCommands = registeredCommands
      .filter((command) => command.startsWith('magit.')).sort();
    assert(registeredCommands.includes('magit.invoke'), 'generic dispatcher is not registered');
    assert.deepEqual(registeredMagitCommands, [
      'magit.cancelEdit', 'magit.closeEdit', 'magit.confirmEdit',
      'magit.invoke', 'magit.key',
      'magit.mirrorFoldShortcuts', 'magit.open',
    ], 'per-action VSCode command registrations exist');
    const contributed = extension.packageJSON.contributes;
    const clientCommands = contributed.commands.map((entry) => entry.command).sort();
    for (const perAction of ['magit.refresh', 'magit.toggle-fold', 'magit.show-section']) {
      assert(!registeredCommands.includes(perAction), `${perAction} must not be registered`);
      assert(!clientCommands.includes(perAction), `${perAction} must not be contributed`);
    }
    for (const serverCommand of ['common.refresh', 'common.toggle-section',
      'common.hide-section', 'common.show-section']) {
      assert(!registeredCommands.includes(serverCommand),
        `${serverCommand} must not be a VSCode command`);
    }
    for (const binding of contributed.keybindings) {
      assert.equal(binding.command, 'magit.invoke', `${binding.key} bypasses the dispatcher`);
      assert.equal(typeof binding.args?.command, 'string', `${binding.key} lacks a server command`);
    }
    const entriesAt = async () => {
      const tokenLegend = await vscode.commands.executeCommand(
        'vscode.provideDocumentSemanticTokensLegend', uri
      );
      const tokenData = await vscode.commands.executeCommand(
        'vscode.provideDocumentSemanticTokens', uri
      );
      const entries = [];
      let current = 0;
      for (let index = 0; index < tokenData.data.length; index += 5) {
        current += tokenData.data[index];
        entries.push({ line: current, type: tokenLegend.tokenTypes[tokenData.data[index + 3]] });
      }
      return entries;
    };
    const dispatchLine = (await entriesAt()).find(
      (entry) => entry.type === 'magit-section' && entry.line > 2
    )?.line;
    assert.notEqual(dispatchLine, undefined, 'no section token for generic dispatch');
    await focusStatus();
    selectLine(dispatchLine);
    const dispatchExpanded = editor.document.getText();
    const dispatchBefore = requests.length;
    await vscode.commands.executeCommand('magit.invoke', {
      command: 'common.hide-section', args: { recursive: true },
    });
    await waitFor(
      () => editor.document.getText() !== dispatchExpanded,
      'generic hide-section dispatch folding a section'
    );
    const dispatchCollapsed = editor.document.getText();
    assert(
      dispatchCollapsed.length < dispatchExpanded.length,
      'generic hide-section dispatch did not fold the section children'
    );
    const hideRequest = requests.slice(dispatchBefore).find(
      (request) => request.method === 'view/invoke'
        && request.params.command === 'common.hide-section'
    );
    assert(hideRequest, 'generic dispatch did not reach view/invoke');
    assert.deepEqual(hideRequest.params.args, { recursive: true },
      'generic dispatch altered the opaque arguments');
    assert.equal(hideRequest.params.viewId, uri.authority);
    assert(hideRequest.params.cursor, 'generic dispatch lost the cursor');
    const dispatchShot = shot('r-vs-3-generic-dispatch.png');
    await vscode.commands.executeCommand('magit.invoke', {
      command: 'common.show-section', args: { recursive: true },
    });
    await waitFor(
      () => editor.document.getText() === dispatchExpanded,
      'generic show-section dispatch restoring the section'
    );
    const showRequest = requests.slice(dispatchBefore).find(
      (request) => request.method === 'view/invoke'
        && request.params.command === 'common.show-section'
    );
    assert.deepEqual(showRequest.params.args, { recursive: true });
    const dispatchRestored = editor.document.getText() === dispatchExpanded;
    assert(dispatchRestored, 'generic show-section did not restore the section');
    const dispatchRestoredShot = shot('r-vs-3-generic-restored.png');
    const foldRequests = requests.filter(
      (request) => request.method === 'view/invoke'
        && request.params.command === 'common.toggle-section'
    );
    const refreshRequests = requests.filter(
      (request) => request.method === 'view/invoke'
        && request.params.command === 'common.refresh'
    );
    assert.equal(foldRequests.length, 4, 'physical Tab/mirror folding did not dispatch four toggles');
    assert.equal(refreshRequests.length, 1, 'physical g did not dispatch one refresh');

    // 8. The Magit-owned QuickPick still forwards typed menu keys.
    await focusStatus();
    selectLine(fileLine);
    const menuText = editor.document.getText();
    const beforeMenu = requests.length;
    await key('c');
    await waitFor(
      () => requests.slice(beforeMenu).some(
         (request) => request.method === 'view/invoke' && request.params.command === 'open.commit'
      ),
      'commit menu open.commit request'
    );
    await waitFor(() => observe().tag === 'INPUT', 'Magit QuickPick input focus');
    await type('-');
    await waitFor(
      () => keysSent().includes('-'),
      'typed menu key reaching view/key'
    );
    await type('a');
    await waitFor(
      () => keysSent().includes('a'),
      'second typed menu key reaching view/key'
    );
    const menuShot = shot('07-magit-quickpick.png');
    await key('q');
    await waitFor(() => keysSent().includes('q'), 'menu q reaching the server');
    await waitFor(
      () => vscode.window.activeTextEditor?.document.uri.toString() === uri.toString(),
      'status focus after the Magit QuickPick closed'
    );
    await waitFor(() => editor.document.getText() === menuText, 'menu pop restoring status text');
    assert(keysSent().includes('q'), 'menu q did not reach the server');
    assert(!invoked('commit.create'), 'typing a menu key ran an action instead of a key step');
    matrix.push({
      widget: 'Magit QuickPick (commit menu)', input: 'c, -, a, q',
      result: 'menu keys reached view/key and q popped back to status',
      magitRequests: requests.length - beforeMenu, screenshot: menuShot,
    });
    interactions.push('Magit commit QuickPick forwarded typed -/a as view/key and q popped to status');

    // 9. F5 is a second canonical refresh default (`profiles/standard.toml`),
    // so its physical behaviour must be recorded honestly rather than assumed.
    // Instrument the isolated renderer to show whether the key reached it and
    // whether a native picker appeared; then press the canonical `g` refresh in
    // the same focus/scope as a control. This never substitutes API dispatch
    // for the physical refresh evidence, which is the `g` press above.
    const f5Binding = contributed.keybindings.find(
      (binding) => binding.key === 'f5' && binding.when.includes("magit.scope == 'status'")
    );
    let f5Shot;
    let f5Note;
    const f5Attempts = [];
    let f5ControlG;
    /**
     * Press physical F5 once and record what the isolated host actually did.
     *
     * @param {string} label
     * @param {string} probe marker line rewritten before the press
     */
    const pressF5 = async (label, probe) => {
      await focusStatus();
      fs.writeFileSync(
        `${vscode.workspace.workspaceFolders[0].uri.fsPath}/tracked.txt`, `${probe}\n`
      );
      await startKeyProbe();
      const before = requests.length;
      await key('F5');
      // Let key resolution finish before reading the probe; the refresh is
      // still awaited through `waitFor`, never through this pause.
      await settle(600);
      const events = stopKeyProbe();
      const dispatched = requests.slice(before).some(
        (request) => request.method === 'view/invoke'
          && request.params.command === 'common.refresh'
      );
      let rendered = false;
      try {
        await waitFor(() => editor.document.getText().includes(`+${probe}`),
          `physical F5 refresh probe (${label})`, 3000);
        rendered = true;
      } catch {
        rendered = false;
      }
      return {
        label,
        probe,
        dispatched,
        rendered,
        keydownsReachingRenderer: events,
        quickInputAfterKey: quickInputState(),
        focus: observe(),
        activeDebugSession: Boolean(vscode.debug.activeDebugSession),
      };
    };
    try {
      // First press without extra input hygiene, so the recorded result matches
      // what a user of this isolated host would see.
      f5Attempts.push(await pressF5('as-is', 'f5-first'));
      shot('08-f5-before-dismissal.png');
      // Dismiss any picker F5 may have opened, then refocus through the API
      // (never with keys).
      await vscode.commands.executeCommand('workbench.action.closeQuickOpen');
      await focusStatus();
      f5Shot = shot('08-f5-probe.png');
      // A second press follows an explicit modulation reset: earlier physical
      // ctrl+alt chords can leave the X layer reporting a modifier that the
      // renderer still treats as held. This changes test input hygiene only.
      try {
        execFileSync('xdotool', ['keyup', 'alt', 'ctrl', 'shift', 'super']);
      } catch {
        // Key names differ across XKB maps; the retry is diagnostic only.
      }
      const clean = await pressF5('after modifier reset', 'f5-second');
      f5Attempts.push(clean);
      await vscode.commands.executeCommand('workbench.action.closeQuickOpen');
      await focusStatus();
      // Control: the canonical `g` refresh must still dispatch in exactly this
      // focus/scope, proving the F5 result is key-specific.
      fs.writeFileSync(`${vscode.workspace.workspaceFolders[0].uri.fsPath}/tracked.txt`, 'g-control\n');
      const controlBefore = requests.length;
      await startKeyProbe();
      await key('g');
      await waitFor(
        () => requests.slice(controlBefore).some(
          (request) => request.method === 'view/invoke'
            && request.params.command === 'common.refresh'
        ),
        'control g refresh dispatch',
        8000
      );
      await waitFor(
        () => editor.document.getText().includes('+g-control'),
        'control g refresh render'
      );
      f5ControlG = {
        dispatched: true,
        rendered: editor.document.getText().includes('+g-control'),
        keydownsReachingRenderer: stopKeyProbe(),
        screenshot: shot('08-f5-control-g.png'),
      };
    } catch (error) {
      f5Note = `F5 probe stopped: ${error && error.message}`;
      await vscode.commands.executeCommand('workbench.action.closeQuickOpen');
      await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
      try {
        f5Shot = shot('08-f5-probe.png');
      } catch {
        f5Shot = undefined;
      }
    }
    const f5Worked = f5Attempts.some((attempt) => attempt.dispatched && attempt.rendered);
    const f5ObservedAttempt = f5Attempts.find(
      (attempt) => attempt.keydownsReachingRenderer.some((event) => event.key === 'F5')
    );
    const f5ArrivedWithAlt = f5Attempts.some(
      (attempt) => attempt.keydownsReachingRenderer.some(
        (event) => event.key === 'F5' && event.alt
      )
    );
    const f5Limitation = f5Worked
      ? 'Physical F5 dispatched and rendered common.refresh in this isolated host.'
      : 'Physical F5 produced no refresh request/render in this isolated host. '
        + `Keydowns reaching the renderer: ${JSON.stringify(
          f5Attempts.map((attempt) => ({
            label: attempt.label, events: attempt.keydownsReachingRenderer,
          }))
        )}. `
        + (f5ArrivedWithAlt
          ? 'The renderer received the key as Alt+F5 (altKey true), so the '
            + 'contributed plain `f5` binding correctly did not match. A '
            + 'disposable xev window reproduced the same host behaviour: '
            + 'xdotool-injected F4/F5/F6 arrive with a preceding Alt_L press, '
            + 'while Tab and g do not. The host component prepending Alt was '
            + 'not modified or conclusively identified.'
          : f5ObservedAttempt
            ? 'The renderer received a plain F5 keydown, so the key was '
              + 'resolved (or swallowed) inside the workbench; the resolver '
              + 'cause remains unproven.'
            : 'The renderer never received an F5 keydown, so the key was '
              + 'consumed before the workbench; the interceptor remains unproven.')
        + ' No programmatic refresh was substituted. R-VS-3 refresh acceptance '
        + 'rests on the canonical `g` binding, physically verified before and '
        + 'again as a control here.';
    interactions.push(
      `F5 probe: ${f5Attempts.map((attempt) => `${attempt.label} dispatched=${
        attempt.dispatched} rendered=${attempt.rendered}`).join('; ')}; `
      + `controlG dispatched=${Boolean(f5ControlG && f5ControlG.dispatched)}`
    );

    // Loaded-manifest evidence for the canonical status keys.
    const bindings = extension.packageJSON.contributes.keybindings.filter(
      (binding) => binding.when.includes("magit.scope == 'status'")
    );
    const canonical = [
      ['enter', 'common.visit'],
      ['tab', 'common.toggle-section'],
      ['g', 'common.refresh'],
      ['f5', 'common.refresh'],
    ].map(([bindingKey, command]) => {
      const binding = bindings.find(
        (candidate) => candidate.key === bindingKey && candidate.args?.command === command
      );
      assert(binding, `loaded manifest is missing ${bindingKey} -> ${command} for status`);
      for (const term of ['magit.active', 'editorTextFocus', '!quickInputVisible',
        '!suggestWidgetVisible', '!findWidgetVisible', '!renameInputVisible']) {
        assert(binding.when.split(' && ').includes(term), `${bindingKey}: missing ${term}`);
      }
      return { key: bindingKey, command, when: binding.when };
    });
    fs.writeFileSync(`${evidence}/r-vs-5-evidence.json`, JSON.stringify({
      uri: uri.toString(),
      text: editor.document.getText(),
      matrix,
      interactions,
      canonical,
      f5: { attempts: f5Attempts, worked: f5Worked, controlG: f5ControlG,
        screenshot: f5Shot, note: f5Note, limitation: f5Limitation },
      tokenTypes: [...types],
      requests,
      tabs: vscode.window.tabGroups.all.flatMap((group) => group.tabs).map((tab) => tab.label),
    }, null, 2));
    // A non-fold canonical action through the same dispatcher must visibly
    // change both the server render and the repository, then restore cleanly.
    await focusStatus();
    const stagedNames = () => execFileSync(
      'git', ['-C', repo, 'diff', '--cached', '--name-only']
    ).toString().trim();
    assert.equal(stagedNames(), '', 'fixture index must start clean');
    const actionBefore = requests.length;
    await vscode.commands.executeCommand('magit.invoke', { command: 'status.stage-all' });
    await waitFor(() => stagedNames() === 'tracked.txt', 'generic stage-all repository effect');
    await waitFor(
      () => editor.document.getText().includes('Staged changes (1)'),
      'generic stage-all server render'
    );
    const stageRequest = requests.slice(actionBefore).find(
      (request) => request.method === 'view/invoke'
        && request.params.command === 'status.stage-all'
    );
    assert(stageRequest, 'generic stage-all did not reach view/invoke');
    assert.equal(stageRequest.params.args, undefined, 'stage-all must carry no args');
    const stageShot = shot('r-vs-3-generic-action.png');
    await vscode.commands.executeCommand('magit.invoke', { command: 'status.unstage-all' });
    await waitFor(() => stagedNames() === '', 'generic unstage-all repository effect');
    const unstageRequest = requests.slice(actionBefore).find(
      (request) => request.method === 'view/invoke'
        && request.params.command === 'status.unstage-all'
    );
    assert(unstageRequest, 'generic unstage-all did not reach view/invoke');
    await waitFor(
      () => editor.document.getText().includes('Staged changes (0)'),
      'generic unstage-all server render'
    );
    const actionRestored = stagedNames() === '';

    // R-EDIT-5: native writable managed drafts against the real server. Native
    // typing/save and the command palette/title actions classify intent before
    // teardown; raw workbench tab close remains ambiguous and must never finish.
    const { store: draftStore } = require(`${extension.extensionPath}/lib/store`);
    const gitDraft = (...args) => execFileSync('git', ['-C', repo, ...args]);
    gitDraft('config', 'user.name', 'Test');
    gitDraft('config', 'user.email', 'test@example.com');
    const head = () => gitDraft('rev-parse', 'HEAD').toString().trim();
    const subject = () => gitDraft('log', '-1', '--format=%s').toString().trim();
    const progressIdle = () => waitFor(() => {
      const active = new Map();
      for (const { token, value } of progressEvents) active.set(token, value.kind);
      return [...active.values()].every((kind) => kind === 'end');
    }, 'native commit progress ended');
    const palette = async (title) => {
      await key('ctrl+shift+p');
      await waitFor(() => quickInputState().visible, 'native command palette');
      await type(title);
      await waitFor(() => quickInputState().text?.includes(title), `palette ${title}`);
      await key('Return');
      await waitFor(() => !quickInputState().visible, `palette command ${title} accepted`);
    };
    let draftProbe = 0;
    const startBridgeEdit = async () => {
      await progressIdle();
      fs.writeFileSync(path.join(repo, 'tracked.txt'), `bridge edit probe ${Date.now()}\n`);
      await focusStatus();
      gitDraft('add', 'tracked.txt');
      assert(stagedNames().length > 0, 'stage bridge edit fixture');
      const prior = new Set(draftStore.drafts.keys());
      await vscode.commands.executeCommand('magit.invoke', { command: 'commit.create' });
      await waitFor(() => [...draftStore.drafts.values()].some((draft) => !prior.has(draft.runId)
        && draft.attached && vscode.window.activeTextEditor?.document.uri.toString() === draft.effect.uri),
      'native managed commit message editor');
      const draft = [...draftStore.drafts.values()].find((record) => !prior.has(record.runId));
      const opens = draftNotes.filter((note) => note.method === 'textDocument/didOpen'
        && note.params.textDocument.uri === draft.effect.uri);
      assert.equal(opens.length, draftProbe + 1, 'one explicit didOpen per reused-path run');
      assert.deepEqual(opens.at(-1).params.textDocument, { uri: draft.effect.uri, languageId: 'gitcommit',
        version: 1, text: draft.effect.text });
      assert.equal(draft.document.getText(), draft.effect.text);
      assert.equal(draft.document.languageId, 'git-commit');
      assert.equal(draft.document.uri.scheme, 'file');
      await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
      await waitFor(() => observe().editorFocused, 'native draft editor focus');
      draftProbe += 1;
      return vscode.window.activeTextEditor;
    };
    const replaceDraft = async (target, text) => {
      const end = target.document.positionAt(target.document.getText().length);
      assert(await target.edit((builder) => builder.replace(new vscode.Range(new vscode.Position(0, 0), end), text)));
    };
    const draftFor = (target) => [...draftStore.drafts.values()].find((record) =>
      record.uri.toString() === target.document.uri.toString());
    const cases = [];
    const bridgeEditor = await startBridgeEdit();
    const commitMessage = `native managed draft ${Date.now()}`;
    const beforeSave = head();
    await replaceDraft(bridgeEditor, '');
    await type(commitMessage);
    await waitFor(() => bridgeEditor.document.getText() === commitMessage, 'native draft typing (including g/q)');
    await key('ctrl+s');
    await waitFor(() => !bridgeEditor.document.isDirty && draftFor(bridgeEditor).savedText === commitMessage,
      'native save checkpoint stays editing');
    assert.equal(head(), beforeSave);
    assert.equal(vscode.window.activeTextEditor.document, bridgeEditor.document);
    shot('r-edit-5-save-stays-editing.png');
    await palette('Magit: Close Commit Draft (Saved Checkpoint)');
    await progressIdle();
    assert.equal(subject(), commitMessage);
    cases.push({ name: 'native-save-normal-close', message: commitMessage, head: head() });

    const recoveryEditor = await startBridgeEdit();
    const unsent = 'native exact unsent\n\nbody ü\n';
    await replaceDraft(recoveryEditor, unsent);
    const record = draftFor(recoveryEditor);
    await waitFor(() => record.text === unsent && !record.flushQueued, 'native recovery text synchronized');
    const checkpointBefore = record.savedVersion;
    assert.equal(checkpointBefore, 1, 'unsaved edits never checkpoint');
    const recoveryUri = record.uri.toString();
    await draftStore.client.sendNotification('textDocument/didChange', {
      textDocument: { uri: recoveryUri, version: record.version }, contentChanges: [{ text: 'stale update' }],
    });
    await waitFor(() => draftMessages.some((effect) => effect.text.includes('stale or duplicated')), 'native readable version error');
    assert.equal(recoveryEditor.document.getText(), unsent);
    shot('r-edit-5-version-error-preserves-text.png');
    assert(await recoveryEditor.document.save(), 'native retry checkpoint succeeds');
    await waitFor(() => record.syncError === undefined, 'native retry sync recovery');
    assert.equal(head(), cases[0].head, 'sync error/retry did not finish');
    await replaceDraft(recoveryEditor, 'native explicit confirm\n');
    await palette('Magit: Confirm Commit Draft');
    await progressIdle();
    assert.equal(subject(), 'native explicit confirm');
    cases.push({ name: 'version-recovery-confirm', head: head(),
      messages: draftMessages.slice(), unsent });
    shot('r-edit-5-confirmed.png');

    // The draft can remain open while status commands go to the status view.
    const outstandingEditor = await startBridgeEdit();
    await focusStatus();
    const statusDispatchBefore = requests.length;
    await vscode.commands.executeCommand('magit.invoke', { command: 'common.refresh' });
    await waitFor(() => requests.slice(statusDispatchBefore).some((request) =>
      request.method === 'view/invoke' && request.params.command === 'common.refresh'),
    'status dispatch while bridge edit remains open');
    assert.equal(vscode.window.activeTextEditor.document.uri.toString(), uri.toString());
    assert.equal(outstandingEditor.document.uri.scheme, 'file', 'native commit file tab remains open');
    await vscode.window.showTextDocument(outstandingEditor.document, { preserveFocus: false });
    const cancelHead = head();
    await palette('Magit: Abort Commit Draft');
    await progressIdle();
    await waitFor(() => vscode.window.activeTextEditor?.document.uri.toString() === uri.toString(),
      'cancel returns focus to status');
    assert.equal(head(), cancelHead, 'explicit abort did not create a commit');
    shot('r-edit-5-aborted.png');

    // Actual dirty Save/Don't Save/Cancel dialogs require a normal host:
    // MAGIT_VSCODE_EDIT_ONLY=1 runs real-file-native.js without the test-host
    // dialog suppression. Do not substitute QuickPick answers for that gate.

    const closeEditor = await startBridgeEdit();
    await replaceDraft(closeEditor, 'native last tab saved close\n');
    assert(await closeEditor.document.save());
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await waitFor(() => vscode.window.activeTextEditor?.document.uri.toString() === uri.toString(),
      'closing the last native commit tab restores status');
    await progressIdle();
    assert.equal(closeEditor.document.uri.scheme, 'file');
    assert.equal(subject(), 'native last tab saved close', 'native last tab close confirms saved disk');
    cases.push({ name: 'native-last-tab-saved-file-close', head: head() });

    const progressEditor = await startBridgeEdit();
    const progressHead = head();
    const token = progressEvents.filter((event) => event.value.kind === 'begin'
      && event.value.message === 'commit').at(-1).token;
    await draftStore.client.sendNotification('window/workDoneProgress/cancel', { token });
    await waitFor(() => progressEvents.some((event) => event.token === token && event.value.kind === 'end'),
      'native commit progress cancellation');
    assert.equal(head(), progressHead);
    await vscode.commands.executeCommand('magit.cancelEdit');
    assert.equal(progressEditor.document.uri.scheme, 'file');
    cases.push({ name: 'progress-cancel', token, head: head() });
    assert.equal(editor.document.uri.scheme, 'magit', 'status remains the read-only content provider');
    fs.writeFileSync(`${evidence}/r-edit-5-evidence.json`, JSON.stringify({
      gate: 'R-EDIT-REAL', cases, requests: requests.filter((request) =>
        request.method === 'workspace/executeCommand'),
      notes: draftNotes, progress: progressEvents,
      closeIntent: 'Actual last-owned-tab closure confirms saved disk; explicit confirm validates synchronized version.',
    }, null, 2));
    fs.writeFileSync(`${evidence}/a07-evidence.json`, JSON.stringify({
      gate: 'A07', adapter: 'realFile',
      confirmedMessage: commitMessage,
      saveOnlyCheckpoints: true,
      statusCommandWithOpenEdit: 'common.refresh',
      statusCommandReachedServer: true,
      cancelCreatedNoCommit: true,
      closeCreatedNoCommit: true,
      closeIntent: 'explicit normal-close versus ambiguous tab teardown',
    }, null, 2));
    fs.writeFileSync(`${evidence}/r-vs-3-evidence.json`, JSON.stringify({
      gate: 'R-VS-3',
      evidenceDirectory: evidence,
      isolatedWindow: { id: windowId, geometry: windowGeometry },
      dispatcher: {
        command: 'magit.invoke',
        registered: registeredCommands.includes('magit.invoke'),
        contributedCommands: clientCommands,
        registeredMagitCommands,
        perActionRegistrations: false,
        actionBindings: contributed.keybindings.length,
        bindingsWithoutDispatcher: contributed.keybindings
          .filter((binding) => binding.command !== 'magit.invoke').map((binding) => binding.key),
      },
      genericDispatch: {
        command: 'common.hide-section',
        args: { recursive: true },
        request: hideRequest.params,
        textChanged: dispatchCollapsed !== dispatchExpanded,
        screenshot: dispatchShot,
        restoredCommand: 'common.show-section',
        restoredArgs: showRequest.params.args,
        restored: dispatchRestored,
        restoredScreenshot: dispatchRestoredShot,
      },
      genericAction: {
        command: 'status.stage-all',
        request: stageRequest.params,
        repositoryStaged: 'tracked.txt',
        screenshot: stageShot,
        restoredCommand: 'status.unstage-all',
        restoredRequest: unstageRequest.params,
        repositoryRestored: actionRestored,
      },
      foldShortcut: {
        key: 'tab',
        binding: canonical.find((entry) => entry.key === 'tab'),
        physicalPresses: 2,
        togglesDispatched: foldRequests.length,
        requests: foldRequests.map((request) => request.params),
        screenshots: [tabFoldShot, '05-status-fold.png', '05b-mirrored-fold.png'],
      },
      refreshShortcut: {
        key: 'g',
        binding: canonical.find((entry) => entry.key === 'g'),
        physicalPresses: 1,
        refreshDispatched: refreshRequests.length,
        requests: refreshRequests.map((request) => request.params),
        renderedLine: '+after-return',
        sameUri: editor.document.uri.toString() === uri.toString(),
        screenshot: '06-status-refresh.png',
      },
      f5: {
        key: 'f5',
        binding: f5Binding,
        attempts: f5Attempts,
        worked: f5Worked,
        controlG: f5ControlG,
        screenshot: f5Shot,
        limitation: f5Limitation,
      },
    }, null, 2));
    fs.writeFileSync(`${evidence}/bindings-evidence.json`, JSON.stringify({
      extensionPath: extension.extensionPath,
      version: extension.packageJSON.version,
      bindings: canonical,
      interactions,
    }, null, 2));
    shot('screen.png');
    await verifyQuitReopen();
    // Last: real native disconnect with an active dirty draft. Do not restart
    // the language client or infer finish from the extension/window lifetime.
    fs.writeFileSync(path.join(repo, 'tracked.txt'), 'native disconnect probe\n');
    gitDraft('add', 'tracked.txt');
    await vscode.commands.executeCommand('magit.invoke', { command: 'commit.create' });
    await waitFor(() => vscode.window.activeTextEditor?.document.uri.scheme === 'file'
      && vscode.window.activeTextEditor.document.uri.path.endsWith('/COMMIT_EDITMSG'), 'native disconnect draft');
    const disconnectEditor = vscode.window.activeTextEditor;
    const disconnectDraft = draftFor(disconnectEditor);
    await waitFor(() => disconnectDraft.attached, 'native disconnect attachment');
    await replaceDraft(disconnectEditor, 'retain native disconnected text\n');
    const disconnectHead = head();
    await require(`${extension.extensionPath}/extension.js`).deactivate();
    assert.equal(head(), disconnectHead);
    assert.equal(disconnectEditor.document.getText(), 'retain native disconnected text\n');
    assert(disconnectDraft.disconnected);
    shot('r-edit-5-disconnected-text-retained.png');
    fs.writeFileSync(`${evidence}/r-edit-5-disconnect.json`, JSON.stringify({
      head: disconnectHead, text: disconnectEditor.document.getText(), disconnected: true,
    }, null, 2));
    console.log(
      'PASS real VSCode R-VS-2 first open/reopen/refresh, '
      + 'R-VS-3 generic dispatcher with physical default fold/refresh and a '
      + 'repository-changing generic action, '
      + 'R-VS-4 server semantic tokens painted by the active theme '
      + '(dark -> light -> dark with identical server text), and '
      + 'R-VS-5 foreign widgets, status Enter/Tab/refresh and Magit QuickPick'
    );
  } catch (error) {
    try { captureFailure?.(); } catch { /* Keep the original verification failure. */ }
    let lifecycle = {};
    try {
      const { store } = require(`${extension.extensionPath}/lib/store.js`);
      lifecycle = {
        activeViewId: store.activeViewId,
        quickPickViewId: store.quickPickViewId,
        views: Array.from(store.views.values()).map((state) => ({
          id: state.id, rootId: state.rootId, uri: state.uri.toString(),
          menu: Boolean(state.quickPick),
        })),
        tabs: vscode.window.tabGroups.all.flatMap((group) => group.tabs).map((tab) => ({
          uri: tab.input?.uri?.toString(), label: tab.label,
        })),
      };
    } catch {}
    fs.writeFileSync(`${process.env.MAGIT_VSCODE_EVIDENCE}/failure.txt`, error.stack);
    fs.writeFileSync(`${process.env.MAGIT_VSCODE_EVIDENCE}/failure-state.json`, JSON.stringify({
      extensionPath: extension.extensionPath,
      version: extension.packageJSON.version,
      window: { id: windowId, geometry: windowGeometry },
      matrix,
      interactions,
      requests,
      renderedLines: Object.fromEntries(renderedLines),
      text: vscode.window.activeTextEditor?.document.getText(),
      lifecycle,
    }, null, 2));
    throw error;
  } finally {
    LanguageClient.prototype.sendRequest = sendRequest;
    LanguageClient.prototype.onNotification = onNotification;
    LanguageClient.prototype.sendNotification = sendNotification;
    LanguageClient.prototype.onProgress = onProgress;
    await require(`${extension.extensionPath}/extension.js`).deactivate();
  }
}

module.exports = { run };
