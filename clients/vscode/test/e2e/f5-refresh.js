'use strict';

const assert = require('node:assert/strict');

/**
 * Press physical F5 on the focused status view and record what the isolated
 * editor host actually did: the editor version, the key/modifier events that
 * reached the renderer, the `common.refresh` request (if any) and the render
 * outcome.
 *
 * The check fails when the loaded manifest contributes the status F5 binding
 * but no physical press dispatched and rendered `common.refresh`. A
 * programmatic refresh is never substituted for the physical key, and the
 * canonical `g` press in the same focus/scope stays as the control.
 *
 * @param {object} context
 * @returns {Promise<object>} recorded probe evidence
 */
async function runF5Probe({ vscode, fs, execFileSync, path, repo, editor, requests, evidence,
  checkpoint, observe, quickInputState, startKeyProbe, stopKeyProbe, key, focusStatus,
  settle, waitFor, contributed }) {
  const binding = contributed.keybindings.find(
    (candidate) => candidate.key === 'f5'
      && candidate.when.includes("magit.scope == 'status'")
      && candidate.args?.command === 'common.refresh'
  );
  const refreshRequests = (start) => requests.slice(start).filter(
    (request) => request.method === 'view/invoke'
      && request.params.command === 'common.refresh'
  );
  const keyTraffic = (start) => requests.slice(start)
    .filter((request) => request.method === 'view/key')
    .map((request) => request.params);
  const renderedProbe = (probe) => {
    const line = editor.document.getText().split('\n')
      .find((candidate) => candidate.includes(`+${probe}`));
    return { rendered: Boolean(line), renderedLine: line || null };
  };

  /**
   * Press physical F5 once and record the observed host behaviour.
   *
   * @param {string} label
   * @param {string} probe marker line rewritten before the press
   */
  const pressF5 = async (label, probe) => {
    await focusStatus();
    fs.writeFileSync(path.join(repo, 'tracked.txt'), `${probe}\n`);
    await startKeyProbe();
    const before = requests.length;
    await key('F5');
    // Let key resolution finish before reading the probe; the refresh is still
    // awaited through `waitFor`, never through this pause.
    await settle(600);
    const events = stopKeyProbe();
    const dispatched = refreshRequests(before);
    try {
      await waitFor(() => renderedProbe(probe).rendered,
        `physical F5 refresh probe (${label})`, 3000);
    } catch {
      // Absence is evidence too; the assertion after the run decides.
    }
    const render = renderedProbe(probe);
    return {
      label,
      probe,
      dispatched: dispatched.length > 0,
      refreshRequests: dispatched.map((request) => request.params),
      keyTraffic: keyTraffic(before),
      rendered: render.rendered,
      renderedLine: render.renderedLine,
      keydownsReachingRenderer: events,
      quickInputAfterKey: quickInputState(),
      focus: observe(),
      activeDebugSession: Boolean(vscode.debug.activeDebugSession),
    };
  };

  const attempts = [];
  let note = '';
  let controlG;
  try {
    // First press without extra input hygiene, so the recorded result matches
    // what a user of this isolated host would see.
    attempts.push(await pressF5('as-is', 'f5-first'));
    checkpoint('f5-before-dismissal.json');
    // Dismiss any picker F5 may have opened, then refocus through the API
    // (never with keys).
    await vscode.commands.executeCommand('workbench.action.closeQuickOpen');
    await focusStatus();
    checkpoint('f5-probe.json');
    // A second press follows an explicit modulation reset: earlier physical
    // ctrl+alt chords can leave the X layer reporting a modifier that the
    // renderer still treats as held. This changes test input hygiene only.
    try {
      execFileSync('xdotool', ['keyup', 'alt', 'ctrl', 'shift', 'super']);
    } catch {
      // Key names differ across XKB maps; the retry is diagnostic only.
    }
    attempts.push(await pressF5('after modifier reset', 'f5-second'));
    await vscode.commands.executeCommand('workbench.action.closeQuickOpen');
    await focusStatus();
    // Control: the canonical `g` refresh must still dispatch and render in
    // exactly this focus/scope, proving the F5 result is key-specific.
    fs.writeFileSync(path.join(repo, 'tracked.txt'), 'g-control\n');
    const controlBefore = requests.length;
    await startKeyProbe();
    await key('g');
    await waitFor(
      () => refreshRequests(controlBefore).length > 0,
      'control g refresh dispatch',
      8000
    );
    await waitFor(
      () => editor.document.getText().includes('+g-control'),
      'control g refresh render'
    );
    controlG = {
      dispatched: true,
      rendered: editor.document.getText().includes('+g-control'),
      refreshRequests: refreshRequests(controlBefore).map((request) => request.params),
      keyTraffic: keyTraffic(controlBefore),
      keydownsReachingRenderer: stopKeyProbe(),
      checkpoint: checkpoint('f5-control-g.json'),
    };
  } catch (error) {
    note = `F5 probe stopped: ${error && error.message}`;
    try {
      await vscode.commands.executeCommand('workbench.action.closeQuickOpen');
      await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
      checkpoint('f5-probe.json');
    } catch {
      // The evidence below records the failure even without a checkpoint.
    }
  }

  const worked = attempts.some((attempt) => attempt.dispatched && attempt.rendered);
  const observed = attempts.find(
    (attempt) => attempt.keydownsReachingRenderer.some((event) => event.key === 'F5')
  );
  const arrivedWithAlt = attempts.some(
    (attempt) => attempt.keydownsReachingRenderer.some(
      (event) => event.key === 'F5' && event.alt
    )
  );
  const passed = !note && Boolean(binding)
    && Boolean(controlG && controlG.dispatched && controlG.rendered) && worked;
  const summary = {
    status: passed ? 'pass' : 'fail',
    editorVersion: vscode.version,
    extensionVersion: require('../../package.json').version,
    binding,
    attempts,
    worked,
    controlG,
    note,
    observedF5Keydown: Boolean(observed),
    arrivedWithAlt,
    checkpoints: ['f5-before-dismissal.json', 'f5-probe.json', 'f5-control-g.json'],
    evidencePath: `${evidence}/f5-evidence.json`,
  };
  fs.writeFileSync(summary.evidencePath, JSON.stringify(summary, null, 2));

  assert.equal(note, '', `F5 probe did not complete: ${note}`);
  assert(controlG && controlG.dispatched && controlG.rendered,
    'control g did not dispatch and render common.refresh in the same focus/scope, '
      + 'so the host is not usable for a key-specific F5 oracle');
  assert(binding, 'the loaded manifest contributes no status f5 -> common.refresh binding');
  assert(worked,
    'physical F5 dispatched no common.refresh request or rendered no refresh on the focused '
      + `status view in this isolated host (editor ${vscode.version}); observed keydowns: `
      + JSON.stringify(attempts.map((attempt) => ({
        label: attempt.label, events: attempt.keydownsReachingRenderer,
      })))
      + `; see ${summary.evidencePath}`);
  console.log(`PASS native F5 refresh on status: ${summary.evidencePath}`);
  return summary;
}

module.exports = { runF5Probe };
