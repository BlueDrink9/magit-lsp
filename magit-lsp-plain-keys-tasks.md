# magit-lsp Plain keys (raw character input): task list

*Former "Level 1". Separate from, and scheduled after, the Plain-client task list (`magit-lsp-plain-client-tasks.md`). Not a release gate. Tasks use the roadmap template (Target Files, Reference Files, Constraints, Acceptance, Verify). Paths marked (new) are proposals. Prepared 29 October 2026.*

## 1. What this is

Plain mode already drives everything through code actions. **Plain keys** adds an optional input path for editors that cannot bind code actions to single keys: the user types one character into the Plain presentation document (insert mode, or replacing a selection), and the server interprets it as the matching Magit key (`s` stage, `u` unstage, `n` next section, and so on), then re-renders the authoritative text, which also removes the typed character.

It is not native key interception, not the existing rich-client text mode, and not a replacement for code actions. Code actions remain the only path for destructive commands.

## 2. Decisions

| Topic | Decision |
|---|---|
| Opt-in | Per-document, off by default. Enabled by a "Keys: on/off" code action, or `plain.keys.default = "on"` in config. A server-rendered header row shows the state. |
| Scope of keys | Single printable characters only, resolved through the existing profile keymap (`standard` or `vim`) and the existing key handler, so menus and chord prefixes work step by step with the view's existing `pending` state. |
| Not supported in v1 | Enter, Tab, Backspace, Escape, modifier keys, and multi-character input. Use code actions for these. |
| Destructive commands | Never run from character input. They are refused with a message pointing to the code action. |
| Classification input | A net edit per `didChange` notification (pre-text hash, range in pre-text, inserted text), not raw `contentChanges` entries. This handles several changes in one notification and clients that send rangeless full text. |
| Patterns accepted | (a) plain single-character insertion; (b) one ranged replace of a selection by one character; (c) Helix-style pair: a deletion, then a single-character insertion at the deletion start in the next notification. Everything else is divergence and is repaired. |
| Never interpreted | Edits in prompt fields (they are field input), edits in any document that is not a Plain presentation document (including the commit message file), edits while keys are off, and edits made while a render or command is in flight. |
| Staleness | A candidate is resolved against the snapshot whose text hash equals the document text before the edit. No match means reject and repair. Mutations are validated by `ActionTarget` as in Plain mode. |
| Liveness | A pending deletion expires after `plain.keys.pair_timeout_ms` (default 250). On expiry the deleted text is restored by a repair render. |
| Known hazard (accepted) | Undoing a server repair can reinsert the removed character, which is indistinguishable from typing it. Accepted because eligible commands are non-destructive and validated against Git. Documented, not defended with heuristics. |

## 3. Preconditions

R-PLAIN-SYNC, R-PLAIN-CORE, R-PLAIN-TARGET, R-PLAIN-RENDER, R-PLAIN-ACTIONS and R-PLAIN-PROMPT are complete. Schedule after R-PLAIN-GATE. The harness (R-PLAIN-HARNESS) gains profiles in R-PKEYS-SPIKE and R-PKEYS-SAFETY.

## 4. Order of work

1. R-PKEYS-SPIKE (gate).
2. R-PKEYS-DELTA, R-PKEYS-POLICY, R-PKEYS-ARM (parallel).
3. R-PKEYS-CLASSIFY.
4. R-PKEYS-EXEC.
5. R-PKEYS-SAFETY.
6. R-PKEYS-E2E.
7. R-PKEYS-SPEC.

## 5. Tasks

### R-PKEYS-SPIKE: record real edit shapes (S, gate)

- **Target Files:** `tests/fixtures/lsp_traces/plain_keys/` (new), `docs/evidence/plain-keys-edit-shapes.md` (new), `tests/common/fake_editor.rs` (new quirk profiles).
- **Reference Files:** the recorded Helix pair trace (delete range, then single-character insert at the next version).
- **Constraints:** recording only; no production code.
- **Acceptance:** raw `didChange` traces, with version numbers, for each of these in plain Neovim (no magit plugin) and in Helix 25.07.1:
  - insert-mode single character;
  - charwise and linewise visual-selection replace;
  - paste of one character and of several;
  - typing several characters quickly (debounce batching in Neovim);
  - undo and redo immediately after a server repair edit.
  - Add harness quirk profiles `selection-replace-pair`, `selection-replace-single`, `rangeless-full-text`, `batched-changes`, `paste-multichar`, `undo-after-repair`, each built from a recorded trace. If a shape is not recorded, mark the fixture "reconstructed".
  - The evidence file states which editors produce which shapes and flags any shape outside the three accepted patterns.
- **Verify:** `make test-rust FILTER="--test plain_harness"`; evidence file reviewed. Do not start later tasks until reviewed.

### R-PKEYS-DELTA: net-edit extraction (M)

- **Target Files:** `src/docstore.rs`, `tests/plain_keys_delta.rs` (new).
- **Constraints:** pure function of (pre-text, notification); no Git, no rendering. Positions use the negotiated encoding.
- **Acceptance:**
  - For each `didChange` notification the store exposes `NetEdit { pre_hash, version_before, version_after, pre_range, inserted }`.
  - One ranged change uses that range. Several ranged changes are composed sequentially and reduced to a net edit, or marked `Complex`. Rangeless changes are diffed against the pre-text by common prefix and suffix.
  - Tests: every recorded fixture from R-PKEYS-SPIKE, multibyte text, CRLF, newline-only ranges, a deletion ending at column 0, empty ranges, and several changes sharing one version.
- **Verify:** `make test-rust FILTER="--test plain_keys_delta"`.

### R-PKEYS-POLICY: key eligibility (S)

- **Target Files:** `src/commands.rs`, `tests/plain_keys_policy.rs` (new), `docs/plain-support.md` (generated table gains a key-input column).
- **Reference Files:** R-PLAIN-PROMPT recording `Ui`; TASKS IMM-6.
- **Constraints:** one `destructive: bool` attribute per command, default false. No allowlist to maintain.
- **Acceptance:**
  - A one-time audit sets `destructive` on every command that discards, hard-resets, deletes or force-pushes.
  - A CI test runs every registered command against the recording `Ui`; any command that emits a destructive confirm prompt without the flag fails the test.
  - Eligibility for character input is: `Implemented`, not `destructive`, and bound in the active profile.
- **Verify:** `make test-rust FILTER="--test plain_keys_policy --test manifest --test parity"`.

### R-PKEYS-ARM: arming, header row, toggle (S)

- **Target Files:** `src/presentation/plain.rs`, `src/config.rs`, `src/commands.rs`, `tests/plain_keys_arm.rs` (new).
- **Acceptance:**
  - Per-document keys state (off by default; `plain.keys.default` overrides).
  - A "Keys: on" / "Keys: off" code action on every Plain view toggles it.
  - The header row is server-rendered; any user edit to it is repaired.
  - State persists across view-stack changes and refreshes, and resets when the document closes.
  - Prompt views and the commit file never arm.
- **Verify:** `make test-rust FILTER="--test plain_keys_arm"`.

### R-PKEYS-CLASSIFY: input classifier state machine (M)

- **Target Files:** `src/presentation/keys.rs` (new), `tests/plain_keys_classify.rs` (new).
- **Reference Files:** the handoff Level 1 state machine (Ready, PendingDeletion, Executing); R-PLAIN-RENDER echo reconciliation.
- **Constraints:**
  - Pure: inputs are `NetEdit`s plus snapshot lookups plus a clock; outputs are `Accept{key, position_or_selection, snapshot}`, `Wait`, or `Reject(repair)`. No Git, no I/O.
  - The classifier acts only when state is Ready, keys are armed, and the pre-edit text hash matches a retained snapshot.
  - Echoes of server edits are recognised by resulting text hash (Plain render rules), never by a flag set while `applyEdit` is outstanding.
- **Acceptance:**
  - Single-character insertion becomes `Accept` with the insertion position as the cursor.
  - A ranged replace by one character, or the delete-then-insert pair at the next version at the deletion start, becomes `Accept` with a selection.
  - Selection conversion: LSP half-open range to inclusive lines; an end at column 0 means the previous line; partial first and last lines and newline-only ranges are covered by tests.
  - A lone deletion becomes `Wait`; it expires to `Reject(repair)` after `plain.keys.pair_timeout_ms`; any other edit, render, other URI or view transition invalidates it.
  - Newline, tab, multi-character, pasted, empty-range and edit-in-header inputs are `Reject`.
  - Space maps to `Space` and `+` to `Plus`; other printable characters map to themselves (shift is never used with printables).
  - Columns are converted from the negotiated encoding to the unit the dispatcher uses; document the unit in a test.
- **Verify:** `make test-rust FILTER="--test plain_keys_classify"`.

### R-PKEYS-EXEC: execution through the dispatcher (M)

- **Target Files:** `src/presentation/plain.rs`, `src/presentation/keys.rs`, `src/session.rs`, `src/keyhandler.rs` (reuse only), `tests/plain_keys_exec.rs` (new).
- **Reference Files:** R-PLAIN-CORE `dispatch`; R-PLAIN-TARGET; TASKS §3.9.
- **Constraints:**
  - Reuse the existing key handler and profile keymap; do not add a second keymap.
  - Resolve the line or selection against the matched snapshot's tree (never by parsing text), then dispatch exactly as `view/key` would.
  - While Executing or Applying, further user edits are dropped and repaired once the render completes. No queueing.
  - Never send keymap pushes to plain clients.
- **Acceptance:**
  - After a command runs, a repair render restores the authoritative text, which also removes the typed character.
  - The render diffs against the document store's current text, not the last-rendered text.
  - Chord and menu steps work one character at a time using the view's `pending` state, and the pending state is shown in the document.
  - An ineligible, destructive, `Planned` or unbound key gives a one-line message and a repair (destructive keys point to the code action).
  - Stale targets give the Plain "changed since displayed; refreshed" message and no mutation.
  - Scenarios: stage file, stage hunk, stage a visual selection, unstage, `n`/`p` navigation, `c` opens the commit menu, then `-a` toggles; each asserts exact Git state.
- **Verify:** `make test-rust FILTER="--test plain_keys_exec"`.

### R-PKEYS-SAFETY: adversarial and property tests (M)

- **Target Files:** `tests/plain_keys_safety.rs` (new), harness profiles.
- **Constraints:** test code only.
- **Acceptance:**
  - A property test generates random edit sequences (insertions, deletions, pastes, undo-like reinsertions, mixed with server renders and echoes) against a real disposable repository. Invariant: Git state changes only for an armed, eligible, correctly shaped candidate against a matching snapshot.
  - Targeted cases: rapid repeated key (second edit dropped during Executing/Applying); echo of a server edit never executes; paste and multi-character input rejected; edits in another URI ignored and not repaired; edits in prompt fields are field input; edits in the commit message file never interpreted; keys off ignores everything; stale pending deletion never attaches to a later insert; undo after repair is repaired and the test documents the accepted reinsertion hazard.
  - A divergence of any kind always ends with the document equal to authoritative text.
- **Verify:** `make test-rust FILTER="--test plain_keys_safety"`.

### R-PKEYS-E2E: native evidence (M)

- **Target Files:** `tests/plain_keys_e2e.rs` (new), `tests/ui/scenarios/plain-keys-*.scn` (new), `docs/evidence/plain-keys-helix-25.07.1.md` (new).
- **Reference Files:** R-PLAIN-E2E plain-Neovim init; `ui-test-plan.md`.
- **Acceptance:**
  - Harness slice on a real repository: enable keys, type `s` on a hunk, `u`, `n`, open the commit menu and toggle a switch, disable keys.
  - Native plain-Neovim scenarios: insert-mode single-key stage; visual-selection replace stage; keys-off typing is repaired; destructive key refused with message. Screens inspected, UI stopped, no orphans.
  - Helix manual checklist: selection replace pairing with a real stage, timeout restore after a lone deletion, keys on and off.
- **Verify:** `make test-rust FILTER="--test plain_keys_e2e"`; `scripts/ui run tests/ui/scenarios/plain-keys-stage.scn`; `scripts/ui run tests/ui/scenarios/plain-keys-visual.scn`.

### R-PKEYS-SPEC: documentation (S)

- **Target Files:** `docs/CLIENT_SPEC.md`, `README.md`, `TASKS.md`.
- **Acceptance:**
  - The spec describes Plain keys as an optional input capability: what a client must send (incremental or full-text `didChange`), the accepted edit patterns, the key set and its exclusions, arming, the timeout, the undo hazard, and that it is never active in prompts or the commit file.
  - The README mentions it as optional, with the limits above.
  - TASKS gains the R-PKEYS entries after the Plain phase, marked post-release.
- **Verify:** documentation review; cross-check against the harness profiles; `git diff --check`.

## 6. Deferred

- Enter, Tab, Backspace and Escape equivalents.
- Destructive commands through character input with a confirm view.
- Multi-character commands.
- Editor-specific read-only presentation to reduce undo noise.

## 7. Risks to watch

1. **Echo versus input.** Misclassifying a server echo as a typed key is the main failure mode; reconcile by resulting text hash.
2. **Pending-deletion liveness.** A lone deletion must always resolve within the timeout, and never pair with a later unrelated insertion.
3. **Undo hazard.** Accepted and documented; non-destructive commands bound the harm.
4. **Debounce and batching.** Several changes per notification are normal (Neovim, Helix decomposition); classification must work on the net edit.
5. **Editor shapes outside the three patterns.** The spike decides whether v1 is viable per editor; unsupported editors keep code actions only.
