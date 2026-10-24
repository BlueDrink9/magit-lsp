'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

/** Requests this test records as evidence; notifications are not captured. */
const WATCHED = new Set(['view/invoke', 'view/key', 'view/open']);

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

/** Verify foreign-widget isolation, Magit QuickPick and restored status keys. */
async function run() {
  const evidence = process.env.MAGIT_VSCODE_EVIDENCE;
  assert(evidence, 'MAGIT_VSCODE_EVIDENCE must name the isolated evidence directory');
  const extension = vscode.extensions.getExtension('magit-lsp.magit-lsp');
  const requests = [];
  const sendRequest = LanguageClient.prototype.sendRequest;
  LanguageClient.prototype.sendRequest = function (method, params, ...rest) {
    if (WATCHED.has(method)) requests.push({ method, params });
    return sendRequest.call(this, method, params, ...rest);
  };
  const magitTraffic = () => requests.length;
  const invoked = (command) => requests.some(
    (request) => request.method === 'view/invoke' && request.params.command === command
  );
  const keysSent = () => requests.filter((request) => request.method === 'view/key')
    .map((request) => request.params.key);
  const matrix = [];
  const interactions = [];
  let captureFailure;
  await extension.activate();
  try {
    const registered = new Set(await vscode.commands.getCommands(true));
    for (const { command } of extension.packageJSON.contributes.commands) {
      assert(registered.has(command), `unregistered contribution: ${command}`);
    }
    await vscode.commands.executeCommand('magit.open');
    const editor = vscode.window.activeTextEditor;
    assert.equal(editor.document.uri.scheme, 'magit');
    assert.equal(editor.document.languageId, 'magit');
    assert.match(editor.document.getText(), /tracked\.txt/);
    const uri = editor.document.uri;
    const first = editor.document.getText();
    await Promise.all([
      vscode.commands.executeCommand('magit.open'),
      vscode.commands.executeCommand('magit.open'),
    ]);
    assert.equal(vscode.window.activeTextEditor.document.uri.toString(), uri.toString());
    assert.equal(vscode.window.tabGroups.all.flatMap((group) => group.tabs)
      .filter((tab) => tab.input.uri?.scheme === 'magit').length, 1);
    assert.equal(editor.document.getText(), first);
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
    for (let index = 0; index < tokens.data.length; index += 5) {
      tokenLine += tokens.data[index];
      tokenEntries.push({
        line: tokenLine,
        type: legend.tokenTypes[tokens.data[index + 3]],
      });
    }
    const types = new Set(tokenEntries.map((entry) => entry.type));
    for (const type of ['magit-section', 'magit-diff-added', 'magit-diff-removed']) {
      assert(types.has(type), type);
    }
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
    const window = execFileSync('xdotool', ['search', '--name', evidence])
      .toString().trim().split('\n');
    assert.equal(window.length, 1, 'expected one isolated host window');
    const windowId = window[0];
    const key = async (name) => {
      // Re-activating an already active Chromium window can move focus away
      // from its find textarea back into the editor's native edit context.
      if (execFileSync('xdotool', ['getactivewindow']).toString().trim() !== windowId) {
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
    const cdp = (method, params) => JSON.parse(execFileSync('node', [
      `${extension.extensionPath}/scripts/test-integration.mjs`, '--cdp',
      process.env.MAGIT_VSCODE_CDP_PORT, method, JSON.stringify(params),
    ], { timeout: 10000, maxBuffer: 16 * 1024 * 1024 }).toString());
    const observe = () => cdp('Runtime.evaluate', {
      expression: `({tag: document.activeElement?.tagName,
        aria: document.activeElement?.getAttribute('aria-label'),
        value: document.activeElement?.value,
        findVisible: Boolean(document.querySelector('.find-widget.visible'))})`,
      returnByValue: true,
    }).result.value;
    const shot = (name) => {
      const path = `${evidence}/${name}`;
      fs.writeFileSync(path, Buffer.from(cdp('Page.captureScreenshot', {
        format: 'png', fromSurface: true,
      }).data, 'base64'));
      return path;
    };
    captureFailure = () => shot('failure-screen.png');
    const focusStatus = async () => {
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
    await vscode.commands.executeCommand('actions.find');
    await waitFor(() => observe().aria === 'Find', 'focused Find textarea');
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
    assert.equal(vscode.window.activeTextEditor.document.uri.toString(), uri.toString());
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
    selectLine(sectionLine);
    const unfolded = editor.document.getText();
    await key('Tab');
    await waitFor(() => editor.document.getText() !== unfolded, 'status Tab folding');
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

    // 6. Returning to status: the canonical refresh binding re-reads Git.
    fs.writeFileSync(`${vscode.workspace.workspaceFolders[0].uri.fsPath}/tracked.txt`, 'after-return\n');
    const beforeRefresh = editor.document.getText();
    const refreshBefore = requests.length;
    await key('g');
    await waitFor(
      () => editor.document.getText().includes('+after-return'),
      'status g refresh',
      15000
    );
    assert.notEqual(editor.document.getText(), beforeRefresh);
    assert(invoked('common.refresh'), 'status refresh did not dispatch common.refresh');
    const refreshShot = shot('06-status-refresh.png');
    matrix.push({
      widget: 'status editor', input: 'g',
      result: 're-read the worktree diff from the server',
      magitRequests: requests.length - refreshBefore, screenshot: refreshShot,
    });
    interactions.push('status g refreshed the worktree diff after foreign widgets');

    // 7. The Magit-owned QuickPick still forwards typed menu keys.
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

    // 8. F5 diagnostic only. The manifest contributes F5 -> common.refresh, but
    // on Linux a native application-menu accelerator can consume the physical
    // key before the renderer sees it. Record what actually happened without
    // gating this ticket, whose refresh evidence uses the `g` binding.
    let f5Dispatched = false;
    let f5Rendered = false;
    let f5Shot;
    let f5Note;
    let f5Focus;
    try {
      await focusStatus();
      fs.writeFileSync(`${vscode.workspace.workspaceFolders[0].uri.fsPath}/tracked.txt`, 'f5-probe\n');
      const f5Before = requests.length;
      await key('F5');
      try {
        await waitFor(
          () => editor.document.getText().includes('+f5-probe'),
          'physical F5 refresh probe',
          3000
        );
        f5Rendered = true;
      } catch {
        f5Rendered = false;
      }
      f5Dispatched = requests.slice(f5Before).some(
        (request) => request.method === 'view/invoke' && request.params.command === 'common.refresh'
      );
      f5Focus = observe();
      shot('08-f5-before-dismissal.png');
      // Dismiss any debug picker the accelerator may have opened, then refocus
      // through the API (never with keys) for the final screenshot.
      await vscode.commands.executeCommand('workbench.action.closeQuickOpen');
      await focusStatus();
      f5Shot = shot('08-f5-probe.png');
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
    interactions.push(
      `F5 probe: dispatched=${f5Dispatched} rendered=${f5Rendered}; refresh acceptance used g`
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
      f5: { dispatched: f5Dispatched, rendered: f5Rendered, focus: f5Focus,
        screenshot: f5Shot, note: f5Note,
        limitation: 'Physical-key diagnostic only; if dispatch is absent, accelerator/resolver cause remains unproven. No programmatic refresh substituted.' },
      tokenTypes: [...types],
      requests,
      tabs: vscode.window.tabGroups.all.flatMap((group) => group.tabs).map((tab) => tab.label),
    }, null, 2));
    fs.writeFileSync(`${evidence}/bindings-evidence.json`, JSON.stringify({
      extensionPath: extension.extensionPath,
      version: extension.packageJSON.version,
      bindings: canonical,
      interactions,
    }, null, 2));
    shot('screen.png');
    console.log('PASS real VSCode R-VS-5 foreign widgets, status Enter/Tab/refresh and Magit QuickPick');
  } catch (error) {
    try { captureFailure?.(); } catch { /* Keep the original verification failure. */ }
    fs.writeFileSync(`${process.env.MAGIT_VSCODE_EVIDENCE}/failure.txt`, error.stack);
    fs.writeFileSync(`${process.env.MAGIT_VSCODE_EVIDENCE}/failure-state.json`, JSON.stringify({
      extensionPath: extension.extensionPath,
      version: extension.packageJSON.version,
      matrix,
      interactions,
      requests,
      text: vscode.window.activeTextEditor?.document.getText(),
    }, null, 2));
    throw error;
  } finally {
    LanguageClient.prototype.sendRequest = sendRequest;
    await extension.exports.deactivate();
  }
}

module.exports = { run };
