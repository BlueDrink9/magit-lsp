'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

/** Verify lifecycle, dispatch and semantic delivery in a real extension host. */
async function run() {
  const extension = vscode.extensions.getExtension('magit-lsp.magit-lsp');
  const invokes = [];
  const sendRequest = LanguageClient.prototype.sendRequest;
  LanguageClient.prototype.sendRequest = function (method, params, ...rest) {
    if (method === 'view/invoke') invokes.push(params);
    return sendRequest.call(this, method, params, ...rest);
  };
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
    assert.match(editor.document.getText(), /tracked.txt/);
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
    // A render notification and provider update may follow the request response.
    const wait = async (condition) => {
      const deadline = Date.now() + 10000;
      while (!(await condition())) {
        if (Date.now() > deadline) throw new Error('Timed out waiting for render/tokens');
        await new Promise((resolve) => setTimeout(resolve, 30));
      }
    };
    await wait(() => editor.document.getText().includes('+replacement'));
    const legend = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokensLegend', uri);
    let tokens;
    await wait(async () => {
      tokens = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokens', uri);
      return tokens?.data.length > 0;
    });
    const types = new Set();
    for (let index = 3; index < tokens.data.length; index += 5) types.add(legend.tokenTypes[tokens.data[index]]);
    for (const type of ['magit-section', 'magit-diff-added', 'magit-diff-removed']) assert(types.has(type), type);
    // Target only this isolated host window, never a user's existing editor.
    const window = execFileSync('xdotool', ['search', '--name', process.env.MAGIT_VSCODE_EVIDENCE]).toString().trim().split('\n');
    assert.equal(window.length, 1);
    const key = async (name) => {
      execFileSync('xdotool', ['windowactivate', '--sync', window[0]]);
      assert.equal(execFileSync('xdotool', ['getactivewindow']).toString().trim(), window[0]);
      execFileSync('xdotool', ['key', '--clearmodifiers', name]);
    };
    const type = (text) => execFileSync('xdotool', ['type', '--window', window[0], '--clearmodifiers', text]);
    const interactions = [];
    await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
    // Verify this ticket before unrelated native-picker scenarios, which have
    // timed out in earlier runs. Physical keys resolve the loaded manifest.
    let tokenLine = 0;
    let foldLine;
    for (let index = 0; index < tokens.data.length; index += 5) {
      tokenLine += tokens.data[index];
      if (legend.tokenTypes[tokens.data[index + 3]] === 'magit-section' && tokenLine > 2) {
        foldLine = tokenLine;
        break;
      }
    }
    assert.notEqual(foldLine, undefined);
    editor.selection = new vscode.Selection(foldLine, 0, foldLine, 0);
    const expanded = editor.document.getText();
    await key('Tab');
    await wait(() => editor.document.getText() !== expanded);
    await key('Tab');
    await wait(() => editor.document.getText() === expanded);
    // Change an already expanded tracked hunk: a newly inserted untracked
    // section may legitimately start folded, hiding its filenames.
    fs.writeFileSync(`${vscode.workspace.workspaceFolders[0].uri.fsPath}/tracked.txt`, 'physical-refresh\n');
    await key('F5');
    await wait(() => editor.document.getText().includes('+physical-refresh'));
    assert.equal(invokes.filter((params) => params.command === 'common.toggle-section').length, 2);
    assert(invokes.some((params) => params.command === 'common.refresh'));
    interactions.push('physical Tab folded/unfolded; physical F5 refreshed the expanded tracked diff');
    const loaded = {
      extensionPath: extension.extensionPath,
      version: extension.packageJSON.version,
      bindings: extension.packageJSON.contributes.keybindings.filter((binding) =>
        ['common.refresh', 'common.toggle-section'].includes(binding.args?.command)),
      registered: [...registered].filter((command) => command.startsWith('magit.')),
      interactions: [...interactions],
      invokes: [...invokes],
      text: editor.document.getText(),
    };
    fs.writeFileSync(`${process.env.MAGIT_VSCODE_EVIDENCE}/bindings-evidence.json`, JSON.stringify(loaded, null, 2));
    execFileSync('import', ['-window', window[0], `${process.env.MAGIT_VSCODE_EVIDENCE}/bindings-screen.png`]);
    fs.writeFileSync(`${vscode.workspace.workspaceFolders[0].uri.fsPath}/tracked.txt`, 'replacement\n');
    await key('F5');
    await wait(() => editor.document.getText().includes('+replacement'));
    await key('ctrl+p');
    // Synchronize with the native widget before sending Enter. The keyboard
    // event can return before the workbench has created its input control.
    await vscode.commands.executeCommand('workbench.action.quickOpen', 'tracked.txt');
    await key('Return');
    await wait(() => vscode.window.activeTextEditor?.document.uri.fsPath.endsWith('/tracked.txt'));
    interactions.push('Ctrl+P plus native quickOpen query and Enter opened tracked.txt over status');
    await vscode.window.showTextDocument(editor.document);
    await key('ctrl+shift+p');
    await vscode.commands.executeCommand('workbench.action.quickOpen', '>File: New Untitled Text File');
    await key('Return');
    await wait(() => vscode.window.activeTextEditor?.document.isUntitled);
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await vscode.window.showTextDocument(editor.document);
    await key('ctrl+p');
    await key('Escape');
    interactions.push('command palette typing/Enter created untitled editor; Ctrl+P Escape dismissed');
    let accepted;
    const foreign = vscode.window.createQuickPick();
    foreign.items = [{ label: 'alpha' }, { label: 'beta' }];
    foreign.onDidAccept(() => { accepted = foreign.selectedItems[0]?.label; foreign.hide(); });
    foreign.show();
    await key('Down');
    await key('Return');
    await wait(() => accepted === 'beta');
    foreign.dispose();
    interactions.push('unrelated QuickPick Down/Enter accepted beta');
    await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
    // Use the server-supplied section token range, not parsed display text.
    let line = 0;
    let sectionLine;
    for (let index = 0; index < tokens.data.length; index += 5) {
      line += tokens.data[index];
      if (legend.tokenTypes[tokens.data[index + 3]] === 'magit-section' && line > 2) {
        sectionLine = line;
        break;
      }
    }
    assert.notEqual(sectionLine, undefined);
    editor.selection = new vscode.Selection(sectionLine, 0, sectionLine, 0);
    const unfolded = editor.document.getText();
    await key('Tab');
    await wait(() => editor.document.getText() !== unfolded);
    await key('Tab');
    await wait(() => editor.document.getText() === unfolded);
    await vscode.commands.executeCommand('magit.invoke', {
      command: 'common.hide-section', args: { recursive: true },
    });
    await wait(() => editor.document.getText() !== unfolded);
    await vscode.commands.executeCommand('magit.invoke', {
      command: 'common.show-section', args: { recursive: true },
    });
    await wait(() => editor.document.getText() === unfolded);
    interactions.push('Tab toggled server section twice; magit.invoke hide/show-section with recursive:true round-tripped through real server');
    const before = editor.document.getText();
    await vscode.commands.executeCommand('magit.invoke', { command: 'common.hide-all' });
    await wait(() => editor.document.getText() !== before);
    await vscode.commands.executeCommand('magit.invoke', { command: 'common.show-all' });
    await wait(() => editor.document.getText().includes('+replacement'));
    fs.writeFileSync(`${vscode.workspace.workspaceFolders[0].uri.fsPath}/new.txt`, 'refresh test\n');
    await key('F5');
    await wait(() => editor.document.getText().includes('new.txt'));
    interactions.push('F5 dispatched common.refresh and displayed newly created new.txt');
    const evidence = process.env.MAGIT_VSCODE_EVIDENCE;
    fs.writeFileSync(`${evidence}/evidence.json`, JSON.stringify({
      uri: uri.toString(), text: editor.document.getText(), types: [...types], interactions,
      tabs: vscode.window.tabGroups.all.flatMap((group) => group.tabs).map((tab) => tab.label),
    }, null, 2));
    execFileSync('import', ['-window', window[0], `${evidence}/screen.png`]);
    console.log('PASS real VSCode lifecycle, duplicate open, dispatch, refresh, folding, semantic delivery');
  } catch (error) {
    fs.writeFileSync(`${process.env.MAGIT_VSCODE_EVIDENCE}/failure.txt`, error.stack);
    fs.writeFileSync(`${process.env.MAGIT_VSCODE_EVIDENCE}/failure-state.json`, JSON.stringify({
      extensionPath: extension.extensionPath, version: extension.packageJSON.version,
      bindings: extension.packageJSON.contributes.keybindings, invokes,
      text: vscode.window.activeTextEditor?.document.getText(),
    }, null, 2));
    throw error;
  } finally {
    LanguageClient.prototype.sendRequest = sendRequest;
    await extension.exports.deactivate();
  }
}

module.exports = { run };
