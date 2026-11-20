# Plain-key edit-shape evidence

## Capture boundary

`make plain-neovim` passed on 2026-11-18 (`ok: build`, `ok: plain-neovim`). The native scenario runs ordinary Neovim typing followed by undo and verifies the authoritative server repair. The actual Neovim log packet for the native insertion/undo sequence is a single versioned `textDocument/didChange`, version 7, containing two ordered edits (insert `NATIVE-DIVERGENCE`, then delete that text). It is retained in `tests/fixtures/lsp_traces/plain_keys/batched-changes.jsonl` with only the temporary file URI redacted. It is **not** evidence for a user selection/paste action; it is a captured batched change within the native test flow.

Helix provenance: `docs/evidence/harness-reassessment-inputs-08.md:1091-1097` retains an actual Helix 25.07.1 version-1 didChange with a one-character insert at 0:0. The capture is retained there verbatim and not relabeled as fresh capture.

The fixture profiles are test-only structural inputs. Profiles other than the captured Neovim batched record are reconstructed; their contents are not claimed as observed editor output. The native Neovim target does not capture all individual editor actions as isolated traces. No scripted native Helix capture exists.

## Editor/action matrix

| Action/shape | Plain Neovim | Helix | Provenance / availability |
|---|---|---|---|
| Single insert | Captured in the native typing sequence, but not isolated as a fixture | Captured, one-character ranged insertion at 0:0, version 1 | Neovim: `make plain-neovim`, native trace; Helix: retained historical capture above. The Helix insert is a single insert. |
| Character-selection replace | Reconstructed: `selection-replace-pair` and `selection-replace-single` | Reconstructed: same profiles | No isolated native selection-replace trace available. |
| Line-selection replace | Reconstructed: `selection-replace-pair` | Reconstructed: `selection-replace-pair` | Pair shape is an explicit design candidate only; neither editor capture available. |
| Paste one character | Reconstructed: `selection-replace-single` | Reconstructed: `selection-replace-single` | No isolated paste capture available. |
| Paste several characters | Reconstructed: `paste-multichar` | Reconstructed: `paste-multichar` | No native capture available. |
| Rapid typing | Reconstructed: `batched-changes` is a native two-change payload, not proof of rapid typing | Reconstructed | No rapid-typing capture available. |
| Undo/redo after server repair | Native target captures insert/undo followed by repair, but not an isolated raw undo/redo packet suitable as this fixture | Reconstructed: `undo-after-repair` | Native evidence establishes scenario sequence, not a user key's individual packet. Redo is not exercised/captured. |
| Delete then one-character insert at same start in next notification | Reconstructed: `selection-replace-pair` | Reconstructed: `selection-replace-pair` | Neither editor's capture of this pair is available. |

## Structural shape audit

The only accepted descriptions are: (1) a single insert; (2) one ranged replace by one character; (3) delete then a one-character insert at the same start in the next notification. The test-only profile audit classifies `selection-replace-pair` as (3), `selection-replace-single` as (2), and `rangeless-full-text`, `batched-changes`, `paste-multichar`, and `undo-after-repair` as **Other / not accepted**. The single-insert form is also directly classified by the harness test, using the retained Helix provenance description. This audit does not implement the T035 classifier or make a production-policy decision.
