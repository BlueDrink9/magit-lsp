# VSCode host behaviours: historical evidence

> Historical observations only; these records do not prove current behavior.
> Text evidence only. Codium versions are stated per observation where known.
> A section that says "current native check" records one structured run at the
> stated base commit and environment; it is not a general host contract.

## Current native check: VSCodium 1.140.06880, magit-lsp extension 0.1.0 (2026-11-19)

Base commit `680cbd1` (focused `vscode-f5` target). Command:
`/opt/env/vscode/bin/xvfb-run -a make vscode-f5` → exit 0. Editor: the
`/opt/env/vscode/bin/codium` build, CLI reports `1.140.06880`
(`807ba2d6075c29587f884698cd7370c81ab0724c`), `vscode.version` `1.140.0`;
extension `0.1.0`. Display: Xvfb with no window manager, so the focused helper
falls back to `xdotool windowfocus --sync` when `windowactivate` has no WM.

Loaded status binding (from the manifest, identical payload to `g`):
`f5` → `magit.invoke { command: "common.refresh" }`, guarded by `magit.active`,
`editorTextFocus`, `!quickInputVisible`, `!suggestWidgetVisible`,
`!findWidgetVisible`, `!renameInputVisible`, `magit.scope == 'status'`,
`!magit.textMode`.

Structured evidence: `/tmp/vscode-review-9Dsasx/f5-evidence.json` with
checkpoints `f5-before-dismissal.json`, `f5-probe.json`, `f5-control-g.json`.
Selected fields, values verbatim:

```json
{
  "status": "pass",
  "observedF5Keydown": true,
  "arrivedWithAlt": false,
  "attempts": [
    {"label": "as-is", "probe": "f5-first", "dispatched": true,
     "refreshRequests": [{"command": "common.refresh", "version": 1}],
     "keyTraffic": [], "rendered": true, "renderedLine": "    +f5-first",
     "keydownsReachingRenderer": [{"key": "F5", "code": "F5", "ctrl": false,
       "alt": false, "shift": false, "meta": false, "repeat": false}]},
    {"label": "after modifier reset", "probe": "f5-second", "dispatched": true,
     "refreshRequests": [{"command": "common.refresh", "version": 2}],
     "keyTraffic": [], "rendered": true, "renderedLine": "    +f5-second",
     "keydownsReachingRenderer": [{"key": "F5", "code": "F5", "ctrl": false,
       "alt": false, "shift": false, "meta": false, "repeat": false}]}
  ],
  "controlG": {"dispatched": true, "rendered": true,
    "refreshRequests": [{"command": "common.refresh", "version": 3}],
    "keyTraffic": [], "keydownsReachingRenderer": [{"key": "g", "code": "KeyG",
      "ctrl": false, "alt": false, "shift": false, "meta": false, "repeat": false}]}
}
```

- Two physical F5 presses on the focused status view each reached the renderer
  as plain `F5` (no Alt and no other modifier) and each dispatched exactly one
  `view/invoke` `common.refresh` (version 1, then 2) with an empty `keyTraffic`;
  each was rendered as the `+f5-first` / `+f5-second` line written before the
  press. F5 therefore works in this host and remains a default binding.
- The second press follows an `xdotool keyup alt ctrl shift super` hygiene step.
  The first press needed no such reset and was already clean, so the reset
  demonstrates nothing about any modifier leakage.

### F5 cause: historical versus current paths

Compared with the 2026-10-24 record above, the editor build is the same
(`1.140.06880`), the extension version and the `f5`/`g` binding payloads are
the same, and both runs inject keys with `xdotool key --clearmodifiers <name>`
into the window owned by the isolated host. The recorded differences are the
display environment (that run had a window manager — windows were floated,
tiled and re-tiled — and asserted `xdotool getactivewindow` before each key;
this run has no WM and uses the `windowfocus` fallback) and the outcome: that run
delivered `altKey: true` F5 preceded by `Alt_L`, this run delivers plain F5.

- What is shown: in the 2026-10-24 display the Alt prefix was already present at
  the X input layer, before editor keybinding resolution — the same injected
  F4/F5/F6 pattern appeared in a disposable `xev` window with no editor
  targeted, while `Tab` and `g` arrived alone. The current identical-build run
  fails to reproduce it, so neither the extension, the contributed binding, the
  `common.refresh` payload nor the editor build is a sufficient cause.
- What is not shown: which component of that display added `Alt`. The historical
  record names no window manager, compositor, global-shortcut daemon, keymap or
  modifier state, and no `xmodmap -pm`, `xset q` or XKB state was captured. The
  cause is therefore unresolved, and the Alt symptom must not be attributed to
  focus handling, a stuck modifier or the test harness's modifier reset: the
  first current press was clean without any reset.
- Proven cause: none. Historical Alt+F5 is an environment-level observation
  that neither the binding nor the client can be blamed for, and it is not
  reproducible on this host.

Concrete evidence still needed to close the cause:

1. Re-run the physical F5 diagnostic in a WM-present session at Codium
   `1.140.06880` (the historical configuration) and capture, in that same run:
   `xset q`, `xmodmap -pm`, an `xkbcomp $DISPLAY -` summary, the window
   manager/compositor name and version, and any running global-shortcut
   service, alongside the existing keydown/request evidence.
2. Reproduce the injection path in isolation: `xdotool key --clearmodifiers F5`
   into `xev` with and without that WM/compositor and with that keymap, to see
   which presence or setting yields the `Alt_L` prefix.
3. Only if a host still delivers Alt+F5 is a client-side decision needed
   (mirror an `alt+f5` binding, or record the limitation and drop F5); do not
   add that binding on current evidence.

## Codium 1.140.06880, magit-lsp extension 0.1.0 (2026-10-24)

- **F5 interception:** Physical F5 reached the renderer as Alt+F5, preceded by
  Alt_L. The plain F5 binding therefore did not dispatch refresh. An isolated
  `xev` window also received F4/F5/F6 with Alt_L, while Tab and `g` arrived
  alone. The responsible host component and interception cause were not proven.
  No native F5 fix is claimed. T105 owns remaining diagnosis.
- **Find-widget focus:** Physical Ctrl+F opened Find, but focus remained in the
  editor's `native-edit-context`; typing went to the read-only status document.
  Invoking `actions.find` instead focused a textarea labelled Find. This was
  observed in Codium 1.140.06880; physical opening-key evidence is distinct
  from the programmatic focus observation.
- **QuickPick focus:** Foreign QuickPick and InputBox accepted typed text with
  zero Magit dispatch. The Magit commit QuickPick accepted `-` and `a` to enable
  `--all`, then `q` returned to status. Observed in Codium 1.140.06880.
- **Fold chords:** In an active status view, Tab dispatched
  `common.toggle-section`; a mirrored Ctrl+Alt+] binding dispatched the same
  fold operation. Both folded and unfolded the server section. Observed in
  Codium 1.140.06880.

## Other version-scoped observation

- Codium 1.140 migrated legacy theme value `Default Dark Modern` to bundled ID
  `Dark Modern`; the exact Codium build was not recorded for this observation.
  A theme test driver waited for semantic token painting rather than relying on
  a fixed delay. This is historical test-driver evidence, not a host contract.

## Historical evidence with unknown Codium version

- An earlier real-client probe found physical Ctrl+F left focus in
  `native-edit-context`, while `actions.find` focused the Find textarea.
  Codium version was not recorded. Its image-capture claims are intentionally
  not migrated; they are not evidence or instructions.
- Earlier failed F5 runs recorded no refresh request after physical F5. Codium
  version was not recorded in those excerpts. Cause remains unproven; T105 owns
  diagnosis.
