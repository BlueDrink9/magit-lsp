# VSCode host behaviours: historical evidence

> Historical observations only; these records do not prove current behavior.
> Text evidence only. Codium versions are stated per observation where known.

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
