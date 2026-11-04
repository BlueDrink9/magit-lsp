# magit-lsp roadmap: numbered tickets in ASD-STE

This document replaces section 7 of TASKS.md. It lists every open ticket. Each ticket has a number and a short name, for example `T008 prompts-and-picks`. The tickets are in phases. The text follows ASD-STE100 style. No automatic STE checker has validated it.

## How to read this document

- A phase is a group of tickets. A phase ends with a phase gate. At the phase gate, run `make hygiene` (after T003 is done). Fix each finding, or make a ticket for it.
- A ticket has these fields: Depends on, Target Files, Reference Files, Constraints, Acceptance, Verify.
- Sizes are S, M or L. They are relative sizes. They are not time estimates.
- Check each path that has the mark (confirm) before you edit it. If the path is wrong, correct the ticket.
- Old ticket numbers (for example R-PLAIN-PROMPT) stay in the history. The last section maps old numbers to new numbers.

## Rules for all tickets

1. **Text evidence only.** Do not make, open or mention an image. Use only text as evidence. Text evidence includes: document text, `.screen` and `.ansi` files, JSON state dumps, wire logs, DOM text dumps, tab dumps, repository state and process lists. A human makes README screenshots with `scripts/readme-shots/`. An agent must not run that script.
2. **Test policy.** CI runs all tests. A full local run is optional. Set `MAGIT_NATIVE=1` to run the native tier on your machine. A feature ticket needs Rust state tests and the conformance scenarios. Use Neovim native evidence only when the visible behavior is new. Use VS Code evidence only when a declared capability changes or a file in `clients/vscode/` changes.
3. **Snapshots and goldens.** Use `UPDATE_EXPECT` only when necessary. A change to a golden needs its own commit. The commit message must explain each changed hunk. A golden must be small and normalized. Add assertions that regeneration cannot change. Do not mark goldens as `linguist-generated`.
4. **Generated files.** Commit a generated file only if a client needs it as shipped. Examples are the keybindings in `package.json`, `.d.ts` files, spec tables and example files. Mark each committed generated file as generated. Use a header, or a `"//generated"` key. Add the file to `.gitattributes` as `linguist-generated`. Add a CI check for drift. Do not commit other generated files.
5. **Delete, do not deprecate.** Keep an option, declaration or variant only if a real consumer uses it. A `keep-for:` comment or `allow(dead_code)` does not give permission to keep dead code.
6. **Fakes.** A fake is trustworthy only if the same assertions also run against the real thing. The expected values must come from a recorded real run. Never fake Git. Each fake capability module must have an `// anchor:` comment. The comment names the real client or trace. Or it says `anchor: none yet`.
7. **Client claims.** Absence of a capability means "not supported" only when the declaration is the feature. These are `window.showDocument`, `workspace.applyEdit`, `semanticTokens` and `workspaceEdit.documentChanges`. For capabilities that only describe sub-features, absence is not a signal. Examples are `definition`, `references` and `executeCommand`.
8. **`applied: true` is not proof.** Neovim returns `applied: true` when it skipped a stale edit. Compare the resulting text hash. Send one document in each `applyEdit`.
9. **Completed tickets.** Shorten a completed ticket to 12 lines or less. Write the requirement, `Done: <commit>` and the test that enforces it. Put the evidence narrative in the commit message.

## Phase map

| Phase | Name | Tickets | Gates 0.2? |
| --- | --- | --- | --- |
| 1 | Review and hygiene | T001 to T003 | Yes |
| 2 | Capability model | T004 to T007 | Yes |
| 3 | Plain interaction | T008 to T010 | Yes |
| 4 | Specification | T011 to T013 | Yes |
| 5 | Commit selection and rebase | T014 to T019 | Yes |
| 6 | Plain acceptance | T020 to T022 | Yes |
| 7 | Test stabilization | T023 to T029 | No (runs in parallel) |
| 8 | Release | T030 to T033 | Starts after phases 1 to 6 |
| 9 | Plain keys | T034 to T039 | No (starts after T022) |
| 10 | Small features | T040 to T047 | No |
| 11 | Read views | T048 to T057 | No |
| 12 | Shared Git workflows | T058 to T073 | No |
| 13 | Advanced workflows | T074 to T098 | No |
| 14 | Deferred | T099 to T111 | No |

Order for phases 1 to 6: do T001 and T002 first, in parallel. Then do T004 and T005. Then do T006 and T007. Phase 3 follows. Phase 4 runs alongside phase 3. Phase 5 runs in parallel with all other phases. T022 closes the gate.

## Closed umbrella tickets

All child tickets of these four tickets are done. Close each umbrella now. Move the remaining work as shown.

| Old ticket | What is done | Remaining work goes to |
| --- | --- | --- |
| R-PLAIN-ACTIONS | `executeCommand` with offers that match a snapshot. Title styles. Cursor, section and selection actions. Boundary and stale-state tests. Refusal of unsupported actions. Descriptions of availability. A latency test. Native content actions. | T008, T009, T010 |
| R-PLAIN-STACK | Menus and Back in one document. Switch toggles. A concrete log. The log to commit menu and back. Native menu and log evidence. Undo repair after a view change. | T008 (menu option rows open prompts) |
| R-PLAIN-CORE | Capability negotiation. A typed delivery seam. One dispatcher. Enforcement of rich optional features. | T009 (stable-ID picker), T010 (provider-file edits), T022 (fake-provider test) |
| R-EDIT-REAL | The real-file cutover. Exact-version confirm and saved-file close. Native close, lifecycle, multi-root and retry evidence in both clients. Ring retention. Same-run recovery. | T104 (lifecycle cases not yet checked) |

## Completed work

| Group | Completed items |
| --- | --- |
| Correctness contracts | IMM-1 to IMM-9 |
| Review fixes | R-COPY-SOURCE, R-NAV, NO-VISION-TESTS, Plain lifecycle and echo fixes |
| Bug fixes | BUG-STATUS-FOLD, BUG-STAGE-NEXT, BUG-STAGED-STAGE, BUG-EDIT-WINDOW, UX-VIM-HORIZONTAL, BUG-READ-1 (minimal slice), Git 2.30 discovery, non-repository message, Plain EOF repair loop |
| Diff safety | R-DIFF-SAFETY and R-PLAIN-TARGET, each with both review corrections |
| Editor bridge | R-EDIT-REAL slices, R-EDIT-2/3/4/5/6/8 (history), stale commit scenarios |
| Commit operations | R-ARGS, R-4.1-head-edit |
| Plain foundation | R-PLAIN-DOC, HARNESS, PREREQ, SYNC, REPO, RENDER, VIS |
| Release and docs | R-CLIENT-SPEC (replaced by T013), R-REL-1a, R-REL-1b, R-REL-2a |

## Phase 1: Review and hygiene (gates 0.2)

### T001 docs-diet (M)

- **T001 migration slices (unchecked):**
  - [ ] T001.1 Map old §7 and review findings to this queue; resolve A11/BUG-READ-1 ownership; write `docs/evidence/task-queue-migration.md`.
  - [ ] T001.2 Apply document rules and ticket-template rules to `AGENTS.md` and `TASKS.md`.
  - [ ] T001.3 Reconcile and shorten completed records; preserve every open issue and its owner.
  - [ ] T001.4 Move host facts, screenshot policy, evidence banners and process-ownership material to their documented homes.
  - [ ] T001.5 Remove superseded historical sections and source planning files only after all open records are represented.
  - [ ] T001.6 Search for image instructions, stale native-evidence wording and unmapped findings; record retained gaps.

- **Depends on:** none.
- **Target Files:** `AGENTS.md`, `TASKS.md`, `ui-test-plan.md`, `docs/SCREENSHOTS.md`, `docs/RELEASING.md`, `clients/vscode/NOTES.md`, `REVIEW_REPORT.md`, `magit-lsp-plain-client-tasks.md`, `magit-lsp-plain-keys-tasks.md`, `docs/evidence/*`.
- **Reference Files:** the Rules section of this document; the current `TASKS.md`.
- **Constraints:** Change text only. Do not change source code. Do not delete the only record of an open issue.
- **Acceptance:**
  1. Put the rules in `AGENTS.md` and in the `TASKS.md` ticket template. Write the no-vision rule in `AGENTS.md` only. Link to it from other files. Add the golden rule: a golden change needs its own commit, and the commit message explains each changed hunk.
  2. Shorten each completed ticket to 12 lines or less. Shorten IMM-1 to IMM-9 to short rules. Name the test that enforces each rule. If no test exists, write "gap" on one line.
  3. Move the host facts from `clients/vscode/NOTES.md` to `docs/evidence/vscode-host-behaviours.md`. Include F5 interception, find-widget focus, QuickPick focus and fold chords. Write the Codium version for each fact. Move "F5 cause not proven" to T105. Delete `NOTES.md`.
  4. Audit `REVIEW_REPORT.md` one finding at a time (A01 to A34). Record the status of each finding and where `TASKS.md` tracks it. Copy each open item to `TASKS.md` as one line. Check A11 and BUG-READ-1 first. Then empty the file. Leave a header and an empty "Open findings" template. List the audited findings in the commit message.
  5. Remove "native evidence in both clients" from the old tickets R-SELECT-OPS, R-4.1a, R-7.3a, R-OP-STATE and R-TODO. Search for other uses of the phrase.
  6. Move screenshot code to `scripts/readme-shots/`. Add `make readme-shots` for human use. Write R-REL-2 again so that screenshots are a manual task. Add a "historical" banner to each evidence file that cites PNG files. Remove PNG mentions from `docs/evidence/real-file-client-spike.md`.
  7. Edit the sections of `TASKS.md`:
     - Top matter: delete the finished "Ad hoc" item and the review checkpoint paragraph.
     - Section 0: replace the "Commit selection (target)" row with the picker rule (see T008). Add rows for test policy, generated files, capability resolution with quirks, and "the rich protocol stays". Merge the seven Plain "(target)" rows into two rows.
     - Sections 3.6 and 3.7: add the prompt presentation principle and the `pick` shape (see T008). Replace "no client picker" with "clients never filter, parse or resolve Git state".
     - Section 3.10: shorten the dated slice entries into the contract. Move the process-ownership text (about 60 lines) to `docs/design/process-ownership.md`.
     - Sections 3.15 and 8.4: replace the token table and the `:TOhtml` plan with the colour tests in T028.
     - Section 9: write the answers. The macOS config directory uses the `dirs` crate (see T047). Escape cancels most views. Full-line tint is wanted (see T094).
     - Sections 10, 11 and 12: delete.
  8. Delete the two Plain task files when this document represents every open ticket in them.
- **Verify:** Run `git diff --check`. Search the documents. No active ticket may tell an agent to capture or open an image. No ticket may say "both clients" for native evidence.

### T002 wire-recorder (S)

- **Depends on:** none.
- **Target Files:** `src/server.rs` or a new `src/wirelog.rs` (confirm), `tests/wirelog.rs` (new), `tests/fixtures/lsp_traces/`.
- **Reference Files:** `docs/evidence/harness-reassessment-inputs.md`; `tests/fixtures/lsp_traces/reconstructed-plain.jsonl`.
- **Constraints:** The recorder is off by default. An environment variable turns it on, for example `MAGIT_LSP_WIRELOG=<path>`. Write JSON lines. Each line has a direction, a time and the raw JSON. Remove home directories and user names. Do not record answers to `secret` prompts or askpass values. Do not write to standard output. Do not change timing.
- **Acceptance:**
  1. The recorder writes both directions in wire order, for requests, responses and notifications. A test replays a recording through the JSON-RPC framing.
  2. Capture one `initialize` and one `didChange` for each of these clients: bundled Neovim, bundled VS Code, plain Neovim and Helix. Record Helix again on its current release. The old 25.07.1 trace did not declare `window.showDocument`. Mark a trace "reconstructed" if you did not capture it. A reconstructed trace does not count as conformance evidence.
  3. For T034, capture these `didChange` shapes from Helix and Neovim: a single insert, a selection replace, a paste of one character, a paste of several characters, rapid typing, and an undo right after a server repair edit. List each shape that you could not capture.
  4. Commit preset files named `initialize-<client>-<version>.json`.
- **Verify:** Run `cargo test --test wirelog`. Make sure that each fixture parses as JSON. Record one capture run for each client as text.

### T003 hygiene-tooling (S)

- **Depends on:** T001.
- **Target Files:** `Makefile`, `AGENTS.md`, `Cargo.toml`, `.luacheckrc` (new), a `knip` configuration in `clients/vscode/` (new), allowlist files (new).
- **Purpose:** Find dead code and file growth at the end of a phase and at review time. Do not run the tools for each ticket.
- **Acceptance:**
  1. `make hygiene` runs these checks. It reports findings only, at first. `cargo machete` (list false positives in `[package.metadata.cargo-machete]`). `knip` for `clients/vscode` (entry points: `extension.js`, tests and scripts). `luacheck` with a `.luacheckrc` that declares the `vim` global. A size report of the 20 largest files that are not generated, with a soft limit. Searches for `allow(dead_code)` and `keep-for:`.
  2. Put each existing finding in an allowlist file. Write a reason for each entry.
  3. `AGENTS.md` has a "Phase end and review" section that names `make hygiene`. The ticket template has a phase gate line. The checklist of T033 has a hygiene entry. Each finding must be deleted, allowlisted with a reason, or put in a ticket.
  4. Optional: run the command each week in CI. Report only.
- **Verify:** `make hygiene` exits with code 0 with the baseline. Add an unused dependency on purpose. Make sure that the tool reports it. Then remove it.

## Phase 2: Capability model (gates 0.2)

### T004 typed-declaration (M)

- **Depends on:** none.
- **Target Files:** `src/presentation/mod.rs`, `src/protocol.rs`, `src/dispatch.rs`, `src/ui.rs`, `src/presentation/custom.rs`, `lua/magit-lsp/config.lua`, `lua/magit-lsp/init.lua`, `clients/vscode/lib/session.js`, `tests/plain_core.rs`.
- **Reference Files:** the declared blocks in `config.lua` and `session.js`; section C of `docs/evidence/harness-reassessment-inputs.md`.
- **Constraints:** Compatibility is not a goal. Delete only code that has no real consumer. Search both clients and the tests before you delete.
- **Acceptance:**
  1. Add a typed struct `MagitCapabilities` (serde, `deny_unknown_fields`). It replaces the hand parsing of `negotiate()` over `Value`. Test both client blocks against it. Delete variants that no client uses: `keys: forwardAll`, `render: full` and `folds: native`. Remove a field from the block if it has only one possible value.
  2. Delete `capabilities.prompts`, `placements` and `editorBridge` from `config.lua`.
  3. Delete this code if no consumer uses it: `PromptUi::multi_choice`, `process_log_id`, `Effect::OpenView` (if only `PushView` is produced), `KeymapDelivery::parse`, `KeyModifiers::parse`, and the duplicate test `RecordingUi`. Keep one shared `RecordingUi`.
  4. Change `ActionTarget` to camelCase (`nodeId`, `mutationHash`). Update the Plain tests and fixtures.
  5. Replace the hard-coded match in `RichCapabilities::command_requirement` with a table. Derive the table from the command requirements. Until the recording `Ui` exists, add a unit test. The test must make sure that each command that produces an effect is in the table.
  6. List the placements that VS Code honors (`views.js`, `effects.js`). Declare only those placements.
  7. Add a checked emitter in the presentation seam. In test builds, an undeclared effect, prompt kind or placement must fail the test. In production, it must use the documented fallback.
- **Verify:** Run `cargo test`, `make test-lua` and `node clients/vscode/test/test.js`.

### T005 quirks-table (M)

- **Depends on:** T002.
- **Target Files:** `src/quirks.rs` and `quirks.toml` (confirm), `src/presentation/mod.rs`, `src/server.rs`, `tests/quirks.rs` (new).
- **Reference Files:** the recorded initialize traces. Neovim behavior: it skips the version check for each `documentChanges` entry after the first. It returns `applied = true` for a stale edit that it skipped.
- **Constraints:**
  - A quirk has up to three effects, all under one id. The effects are a capability override (add or remove), a named server workaround, and a named fake-editor behavior profile. A shared quirk applies to many editors when each editor lists the id.
  - Match by `clientInfo.name` and version range, or by a capability test. An example of a capability test is "declares `experimental.magit`". Plain Neovim and bundled Neovim both report `Neovim`. VS Code always reports `Visual Studio Code`.
  - Resolve in this order: declared or derived capabilities, quirk overrides, user config. User config can turn a quirk on or off. The field `experimental.magit.effective` reports the result.
  - Apply a cheap workaround that is always correct to all clients. Do not put it behind a quirk. Echo-hash reconciliation is an example.
- **Acceptance:**
  1. Add these entries: `assume-document-changes` and `apply-result-unreliable` for Neovim; `decomposed-changes` for Helix; `assume-show-document` for Helix builds that handle `showDocument` without a declaration. Enable the last entry only after T002 records the evidence.
  2. Each quirk that has a `model` has a fake-editor test. Each quirk that has a `workaround` has a server test. An unknown client gets no quirks.
  3. Generate a "Known client behaviors" appendix for the client spec (T013).
- **Verify:** Run `cargo test --test quirks`.

### T006 fake-editor-model (L)

- **Depends on:** T002, T004, T005.
- **Target Files:** `tests/plain_harness.rs`, `tests/common/fake_editor.rs`, a new `tests/common/client_pump.rs` and `tests/common/editor_model/` (confirm).
- **Reference Files:** the current `ClientPump` and its 12 tests; `tests/data/client-contracts.json`; the recorded Helix and Neovim traces.
- **Constraints:** The old ticket R-PLAIN-HARNESS is done. Its tests must still pass. A scripted server request in an old test is not conformance evidence.
- **Acceptance:**
  1. Move `ClientPump` from `plain_harness.rs` to `tests/common/`. Let a test set the initialize capabilities (a rich block or standard capabilities only). Allocate request ids dynamically. The fake must not use the fixed ids 71 and 72.
  2. A client model is a base plus modules. The bases are `plain-min` (only `applyEdit` and `codeAction`) and `rich-min` (core render and key, `layout:[main]`, no optional effects or prompts). Each module has a `declares` part, a `behaves` part and an `// anchor:` comment.
  3. Add strict mode. A test fails if the server uses a feature that the model did not declare.
  4. Generate the behavior profiles from the quirks table. Keep the existing profiles. Add `silent-stale-apply` and `neovim-accepts-undeclared-documentChanges`.
  5. Add a rich view model. It applies `view/render` (full, edits and metadata-only), sends acks, resyncs, and answers prompts from the shared vectors.
  6. Add presets from recorded traces: Neovim stock, Helix, bundled Neovim and bundled VS Code. Add three named profiles: `rich-native`, `rich-doc` and `plain`.
  7. Add a test where the real server sends `workspace/applyEdit` after a Plain `didOpen`, and the pump answers it.
- **Verify:** Run `make test-rust FILTER="--test plain_harness"`.

### T007 plain-capability-fixes (M)

- **Depends on:** T005, T006.
- **Target Files:** `src/presentation/mod.rs` (`from_lsp`, `content_action`), `src/presentation/plain.rs`, `src/server.rs`, `tests/ui/plain_init.lua`, `tests/test_magit_lsp.lua`.
- **Reference Files:** the recorded Neovim and Helix initialize traces; the Neovim `util.lua` source and its `workspace/applyEdit` handler (this evidence comes from source reading; the mini.test below confirms it).
- **Constraints:** Do not add a new negotiation mechanism. Send one document in each `applyEdit`.
- **Acceptance:**
  1. Allow Plain actions when the client declares `textDocument.codeAction`. Remove the `executeCommand` requirement from `from_lsp` and `content_action`. You can then remove the workaround from `plain_init.lua`.
  2. Treat Neovim as able to apply `documentChanges`. Use the quirk from T005.
  3. For each capability that `from_lsp` reads, write a code comment. The comment says if the declaration is the feature, or only describes sub-features.
  4. Add an echo watchdog. If `applyEdit` returns `applied: true` and no echo arrives in a fixed time, the document must not stay in `Applying`. Compare it with the next snapshot, or render again. Test this with `silent-stale-apply`.
  5. Add a mini.test for the server-to-client `workspace/applyEdit` path. Use an in-process Lua server (`vim.lsp.start{cmd = function(dispatchers) ... end}`). Test four cases: the client declares `documentChanges` (yes or no) and the edit version matches or is stale. A matching edit changes the buffer. A stale edit does not. `applied` is `true` in all four cases. Add a case with two `documentChanges` entries where the second entry is stale. Read the real `changedtick` after attach. Keep stale versions positive.
  6. For a client that does not declare `window.showDocument`, show the commit message by hand. The server sends a message and a document row with the `COMMIT_EDITMSG` path. It accepts a `didOpen` of that URI as the editor attach. It offers Confirm and Abort code actions. Test this with one harness scenario that uses the `no-showDocument` profile. Do not add more tests. A client that declares `showDocument` uses it.
- **Verify:** Run `make test-rust FILTER="--test plain_core --test plain_harness --test plain_render"` and `make test-lua`.

## Phase 3: Plain interaction (gates 0.2)

### T008 prompts-and-picks (L)

- **Depends on:** T004, T007. Old ticket: R-PLAIN-PROMPT (with the pick ladder and the option rows of R-PLAIN-STACK).
- **Target Files:** `src/presentation/prompt.rs` (new), `src/tree.rs` (Field node), `src/server.rs` (`completionProvider`), `src/commands.rs`, `src/protocol.rs`, `src/ui.rs`, `lua/magit-lsp/prompt.lua`, `clients/vscode/lib/prompt.js`, `clients/vscode/lib/requests.js`, `tests/data/client-contracts.json`, `docs/plain-support.md` (generated), `tests/plain_prompt.rs` (new).
- **Reference Files:** `TASKS.md` sections 3.5 and 3.6; sections C and D of `docs/evidence/harness-reassessment-inputs.md`; the current `PromptUi` and the recording `Ui`.
- **Constraints:**
  - A `Field` is a tree node with a tracked range. Read only the text that the user typed from the document store. Never read structure from text. Repair edits outside fields. A field is one line and has a size limit.
  - A client must never filter, parse or resolve Git state.
  - `secret` prompts and askpass work only in rich clients. A Plain client cancels them and shows a message.
  - Use static pick delivery only. T106 covers paged and query delivery.
- **Acceptance:**
  1. Add the prompt presentation principle to section 3.6. A prompt is semantic. Each client chooses the presentation. A prompt kind that the client did not declare uses the document renderer. The document renderer works over both transports (`view/render` and `applyEdit`).
  2. Add document prompts for all four kinds. `confirm`: Yes and No rows. `text`: a field with Submit and Cancel. `choice`: item rows, and a field when `allowCustom` is true. Put the default in the field. `multiChoice`: toggle rows and a Done row. Always show the rows. `textDocument/completion` on fields (branches, tags, remotes, revisions) is an extra. Do not depend on it.
  3. Add a `pick` declaration to the typed capability block. Its fields: `purpose`, `multi`, `allowFreeText`, preselection, items `{id, label, description?, detail?, group?}` and an optional `fallbackView`. The order of use: native pick (declared), server-rendered view, code actions over a document. The server must not send a shape that the client did not declare. One side filters each pick.
  4. Apply this rule. A target that you can reach from HEAD uses log select. All other targets use a text pick that accepts free text. Check free text on the server with `git rev-parse --verify`. The value at the cursor is a preselection. The picker always appears. For a multi-selection, preselect more than one item. An item may open the commit in the log. IMM-6 (the smallest node under the cursor) does not change.
  5. Replace the current behavior for an undeclared prompt kind (a warning and a dead command) with the document renderer. A prompt view holds refreshes and repairs until the prompt ends. The text of a field is the current text in the document store. A diff must never change it.
  6. A user preference inside the declared limit wins. To go beyond the limit, the user needs an explicit config key for that client (by `clientInfo.name`). Without the key, use the fallback and show a message.
  7. Plain discard uses the document confirm. This replaces `Unavailable`. Menu option rows open prompts.
  8. Fix two Lua bugs. First, a custom `choice` entry calls `ask_text`, which puts `params.default` (an item id) in the field. Second, an empty `inputsecret` result cancels in Lua, but VS Code returns an empty string. Choose one behavior. Test both clients with the shared vectors.
  9. Neovim declares static picks only (`vim.ui.select`). Cut long lists with a marker, or use the document renderer. Check each pick again at execution time.
  10. A test runs each registered command against the recording `Ui`. It generates `docs/plain-support.md`, keyed by effective capabilities and reasons. A command that sends an unsupported effect becomes `Unavailable{reason}`. An unsupported effect at runtime gives a readable message and a clean cancel.
  11. Add a mixed-rung test. In each step, the client handles the prompt natively or by document. Cancel at each step. Make sure that nothing changes, the caller returns, and the view stack depth is the same.
- **Verify:** Run `make test-rust FILTER="--test plain_prompt --test manifest --test parity"`, `make test-lua` and `node clients/vscode/test/test.js`.

### T009 stable-id-selection (M)

- **Depends on:** T008. Old ticket: R-PLAIN-SELECT.
- **Target Files:** `src/presentation/plain.rs`, `src/commands.rs`, `src/session.rs`, `tests/plain_select.rs` (new).
- **Reference Files:** `TASKS.md` section 3.7; the log select implementation from R-5.1.
- **Constraints:** Build on the `pick` abstraction from T008. Do not make a second mechanism. A commit identity comes from structured nodes. Never read it from text.
- **Acceptance:** The log select view works as a normal Plain document. Confirm, cancel and toggle-mark are code actions. A free-text revision uses a field. The commit identity comes from `ActionTarget`. Confirm and cancel both return the caller. A stale selection is rejected and changes nothing. Scenario: select a fixup target. Then, in a second run, cancel. The second run must change nothing.
- **Verify:** Run `make test-rust FILTER="--test plain_select"`.

### T010 plain-commit-editing (S)

- **Depends on:** T007. Old ticket: R-PLAIN-DRAFT.
- **Target Files:** `src/presentation/plain.rs`, `src/editor.rs`, `tests/plain_draft.rs` (new).
- **Reference Files:** `TASKS.md` section 3.10 (the real-file contract); T007 item 6.
- **Constraints:** Reuse the real-file workflow and the `magit.command` confirm. Identify `COMMIT_EDITMSG` by its canonical path and URI. Never use `languageId`. Do not add a new draft protocol.
- **Acceptance:**
  1. For `editDocument` when the client declares `showDocument`: if the document store shows that the URI is open, send a versioned `applyEdit` that replaces the whole buffer with the new template. Then send `showDocument` with `takeFocus`. If the URI is not open, send `showDocument` only.
  2. When the client does not declare `showDocument`, use the manual path from T007 item 6.
  3. If no `didOpen` arrives in a fixed time, cancel the run and show a message. Mark the commit commands `Unavailable`.
  4. Confirm is a code action. It uses the latest synchronized content at the exact version. A close confirms the saved file. Abort is a code action. It runs `common.cancel {runId}` and shows the status view. Ignore a close that arrives after an abort. Reject unrelated documents.
  5. Test these cases: save then close; dirty Save; dirty Discard (uses the prior saved content); dirty Cancel (the buffer stays open); empty text saved (Git aborts); explicit Confirm at the exact version; explicit Abort (no commit); a stale buffer; a second session gets no actions.
- **Verify:** Run `make test-rust FILTER="--test plain_draft --test editor_bridge"`.

## Phase 4: Specification (gates 0.2)

The client spec is a main deliverable. A person must be able to read it and implement a client, from basic LSP setup to a rich client.

### T011 schema-generation (M)

- **Depends on:** T004.
- **Target Files:** `src/protocol.rs`, `src/menu.rs`, `src/view.rs`, `src/dispatch.rs`, `src/editor.rs`, `Cargo.toml`, a generator binary or xtask (new), `docs/schema/` (generated), `clients/vscode/types/*.d.ts` (generated), `lua/magit-lsp/types.lua` (generated).
- **Reference Files:** section C of `docs/evidence/harness-reassessment-inputs.md` (the serde hazards).
- **Constraints:** Handle these hazards:
  - `ViewKind` has a custom `Serialize`. Use `#[schemars(with = "String")]` or write the schema by hand.
  - `menu.rs` uses `serialize_with` and `flatten`. Annotate the item list.
  - Change `PlainActionTarget` to an internally tagged enum. The prompt value types stay as `anyOf`.
  - `EditConfirmation` is serialize-only. `EditConfirmParams` is deserialize-only. Generate each in its own mode.
  - `Option<Value>` fields stay as `any`.
  - Methods are trait impls, not data. Write a small method table by hand: method, direction, kind, params type, result type.
- **Acceptance:**
  1. Generate a JSON Schema from the Rust types and the method table. Commit it in `docs/schema/`. Add a drift check in CI.
  2. Generate a `.d.ts` file. Check the JavaScript with `tsc --checkJs` in `make vscode-test`.
  3. Generate LuaLS `---@class` annotations.
  4. Generate the payload tables for the client spec from the schema.
  5. A test checks each recorded fixture and each example file against the schema.
  6. Only the effect list and the prompt list are the same in both clients. Generate those two from the typed block. Keep the key translators and token mappings separate.
- **Verify:** Run the schema drift check, `make vscode-test` and the Rust validation test.

### T012 example-files (S)

- **Depends on:** T011.
- **Target Files:** `docs/examples/<level>-<name>.jsonl` (new), `tests/examples.rs` (new).
- **Constraints:** Each line is `{dir, message, note?, source}`. `dir` is `c2s` or `s2c`. `source` is `recorded` or `written`. These files are the only source of JSON in the client spec. Do not write JSON by hand in the spec text.
- **Acceptance:** Each message is valid against the schema. A harness test sends the client-to-server lines to the real binary and compares the normalized server lines. Each example is also a golden. The first set has these examples: minimal initialize and Plain open; a code-action stage; a document prompt; the rich open, render, ack and key loop with progress order; a commit edit confirm and abort.
- **Verify:** Run `cargo test --test examples`.

### T013 client-spec (L)

- **Depends on:** T011, T012. Start the skeleton now. Add content as T005, T006, T007 and T008 finish. Old tickets: R-PLAIN-SPEC and R-CLIENT-SPEC.
- **Target Files:** `docs/CLIENT_SPEC.md`, `README.md`.
- **Reference Files:** `src/protocol.rs`; the schema; the quirks table; the example files; the results of T023 when available.
- **Constraints:** Generate payload tables and JSON. Do not write them by hand. Check each claim against `src/protocol.rs` and a test. Do not mix current behavior and target design on one page. Give each feature a status and the test that proves it.
- **Acceptance:**
  1. The spec has five levels. Each level is a complete working client. Each level has: what the client sends and receives; the capabilities to declare and what fails without them; one worked exchange from `docs/examples`; and a conformance checklist keyed to scenario ids.
  2. Level 0: connect, initialize, document sync and position encodings. Level 1: Plain read-only (the status document, `applyEdit`, echo handling). Level 2: Plain actions (code actions, `executeCommand`, selections). Level 3: Plain extras (document prompts, completion, `showDocument` and the manual commit path, progress). Level 4: rich (render, ack, resync, key delivery, effects, native prompts and picks, menus, text mode, real-file drafts).
  3. Add a table from capabilities to the level of support. Add the generated "Known client behaviors" appendix from the quirks table. Add the rule for capability classification. Add the Neovim `applyEdit` and `documentChanges` facts.
  4. Use `tests/ui/plain_init.lua` (30 lines) and a Helix TOML file as reference clients.
  5. In `README.md`, state that a general LSP client works in Plain mode when it follows the spec. Name only clients that were tested.
- **Verify:** Check the examples and links. Make sure that the generated sections are current. Review the document.

## Phase 5: Commit selection and rebase (gates 0.2)

Interactive rebase stays on the release gate. Use Neovim for native evidence in this phase. Plain acceptance comes from the harness in T022.

### T014 selected-commit-continuation (M)

- **Depends on:** none (R-ARGS and R-5.1 are done). Old ticket: R-SELECT-OPS.
- **Target Files:** `src/session.rs`, `src/commands.rs`, the log select view (confirm), `tests/commands.rs`, `tests/ui/scenarios/commit-select-*.scn` (new).
- **Reference Files:** `TASKS.md` section 3.7; the current log select code and the `PromptAction` continuation.
- **Constraints:** The identity of a selection is the full commit id from the structured node. Never use rendered text or a row index.
- **Acceptance:** The full selected commit id, the captured operation arguments and the caller all survive the select-mode continuation. Tests cover confirm, cancel and a stale selection.
- **Verify:** Run `cargo test --test commands` and `scripts/ui run tests/ui/scenarios/commit-select-*.scn`.

### T015 fixup-action (M)

- **Depends on:** T014. Old ticket: R-4.1a.
- **Target Files:** `src/commands.rs`, the commit menu registry, `tests/commands.rs`, `tests/ui/scenarios/commit-fixup.scn` (new).
- **Reference Files:** `docs/evidence/commit-head-operations.md`; the fixup section of the Magit manual.
- **Constraints:** Use the server-owned log select. Do not use a client picker. Other menus and actions do not gate the release.
- **Acceptance:** Tests of the exact repository state show that the selected commit is the fixup target. Cancel changes nothing and returns the caller. Confirm returns to the caller. Failures are readable.
- **Verify:** Run `cargo test --test commands` and `scripts/ui run tests/ui/scenarios/commit-fixup.scn`.

### T016 editor-helper-kinds (S)

- **Depends on:** none. Old ticket: R-EDIT-KINDS.
- **Target Files:** `src/editor.rs`, `tests/editor_bridge.rs`.
- **Constraints:** A todo run must not use the commit history or the close rules of a commit message.
- **Acceptance:** The helper checks each kind (commit message and todo) against the expected target. Each kind has its own policy. A wrong kind or wrong target is rejected.
- **Verify:** Run `cargo test --test editor_bridge`.

### T017 rebase-state (M)

- **Depends on:** T016. Old ticket: R-OP-STATE.
- **Target Files:** `src/git.rs`, `src/status_view.rs`, `src/commands.rs`, `tests/rebase_state.rs` (new), `tests/ui/scenarios/rebase-state.scn` (new).
- **Reference Files:** the in-progress rebase fixtures in `scripts/ui_lib/fixtures.py`.
- **Acceptance:** The server finds the rebase state and the operation in progress. The server represents and reports these outcomes safely: continue, skip, abort and conflict.
- **Verify:** Run `cargo test --test rebase_state` and `scripts/ui run tests/ui/scenarios/rebase-state.scn`.

### T018 rebase-todo (M)

- **Depends on:** T016, T017. Old ticket: R-TODO.
- **Target Files:** a new `src/rebase_todo.rs` (confirm), `src/view.rs`, `tests/rebase_todo.rs` (new), `tests/ui/scenarios/rebase-todo.scn` (new).
- **Constraints:** The server parses and writes the todo. Reject unsupported grammar safely. Full rebase-merges support is T079.
- **Acceptance:** The server parses, edits, reorders, checks and writes the linear todo. A todo that is not valid, or uses unsupported grammar, is rejected. The file does not change.
- **Verify:** Run `cargo test --test rebase_todo` and `scripts/ui run tests/ui/scenarios/rebase-todo.scn`.

### T019 interactive-rebase (L)

- **Depends on:** T014, T016, T017, T018. Old ticket: R-7.3a.
- **Target Files:** `src/commands.rs`, the rebase menu registry, `tests/rebase_interactive.rs` (new), `tests/ui/scenarios/rebase-interactive.scn` (new).
- **Constraints:** The server owns the selection and the structured todo. Other history and branch features do not gate the release.
- **Acceptance:** Tests of the exact repository state cover: the selected base, execution of the edited todo, continue, abort and the conflict state. Cancel before execution changes nothing. Failures are readable.
- **Verify:** Run `cargo test --test rebase_interactive` and `scripts/ui run tests/ui/scenarios/rebase-interactive.scn`.

## Phase 6: Plain acceptance (gates 0.2)

### T020 plain-e2e (L)

- **Depends on:** T009, T010. Old ticket: R-PLAIN-E2E.
- **Target Files:** `tests/plain_e2e.rs` (new), `scripts/ui_lib/*`, `scripts/test_plain_neovim.py` (fold it in, then delete it), `tests/ui/plain_init.lua`, `tests/ui/scenarios/plain-status-stage.scn` and `plain-commit.scn` (new), `ui-test-plan.md`.
- **Reference Files:** `scripts/ui_lib/cli.py`, `driver.py`, `scenario.py`; section A.5 of `ui-test-plan.md`.
- **Constraints:** Read `scripts/test_plain_neovim.py` before you change it. The init file uses `vim.lsp.start` and no code from `lua/magit-lsp`. The header `profile:` in a `.scn` file already means the keymap profile. The new header is `client: rich | plain`.
- **Acceptance:**
  1. Fold `test_plain_neovim.py` into `ui_lib` as `client: plain` (init file and environment). A Plain scenario and a rich scenario are then two `.scn` files in one harness.
  2. Add a harness test on a disposable repository: open, status, stage a file, stage a hunk, a stale action is rejected, unstage, commit menu, commit file, save, close. The commit must exist with the exact message.
  3. Add native plain-Neovim scenarios for status and stage, and for commit editing. Run them at 80x24. Record text evidence. No process may stay running.
- **Verify:** Run `make test-rust FILTER="--test plain_e2e"` and `scripts/ui run tests/ui/scenarios/plain-status-stage.scn tests/ui/scenarios/plain-commit.scn`.

### T021 helix-smoke (S)

- **Depends on:** T010. Old ticket: R-PLAIN-HELIX.
- **Target Files:** `docs/evidence/plain-helix.md` (new).
- **Reference Files:** the Helix TOML in the client spec.
- **Constraints:** Use the generic configuration only. Do not use editor-specific expansions. This is a checklist. It is not a test suite.
- **Acceptance:** Record the Helix version, the date, and whether Helix declared `window.showDocument`. Complete this checklist: open, status, stage, menu, log, commit (use `showDocument` if declared, or the manual path), abort. Make a ticket for each failure.
- **Verify:** Review the evidence file.

### T022 plain-gate (S)

- **Depends on:** T006, T013, T015, T019, T020, T021. Old ticket: R-PLAIN-GATE.
- **Target Files:** `tests/plain_e2e.rs`, `docs/plain-support.md`.
- **Acceptance:**
  1. Run the T015 and T019 scenarios through the fake-editor harness. Assert the exact repository state.
  2. Add a minimal fake provider. It must prove that the dispatcher and the view interaction work without Git. Do not build a jj backend.
  3. Generate `docs/plain-support.md` again. No command may be `Unavailable` without a reason.
  4. Record the results of `make test`, `make ui-test` and `make hygiene`.
- **Verify:** Run `make test`, `make ui-test` and `make test-rust FILTER="--test plain_e2e"`.

## Phase 6A: Rich protocol v1 and client prototypes

The rich protocol stays. Make it stable early. Before the final freeze, build prototypes for Visual Studio, Zed and JetBrains. A prototype shows which parts of the protocol every client needs. Visual Studio means the Microsoft Visual Studio IDE, not VS Code.

0.2 ships the rich protocol as "v1 preview". Inside v1 preview, only additive changes are allowed. T105, T112, T113, T114 and T119 gate 0.2. T115, T116 and T117 start after T113. T118 freezes v1 as stable. T118 does not gate 0.2.

A second backend (for example jj) reuses the views, key bindings and protocol. It adds new views and commands. It does not need a protocol break or a hard fork (see T046 and T098).

### T112 rich-protocol-audit (S)

- **Depends on:** none.
- **Target Files:** `docs/design/rich-protocol-audit.md` (new). Read these files: `src/protocol.rs`, `src/session.rs`, `src/server.rs`, `lua/magit-lsp/view.lua`, `lua/magit-lsp/buffer.lua`, `clients/vscode/lib/render.js`, `clients/vscode/lib/requests.js`, `clients/vscode/lib/effects.js`.
- **Reference Files:** `docs/CLIENT_SPEC.md`; `docs/evidence/harness-reassessment-inputs.md`; `TASKS.md` sections 3.3, 3.6 and 3.9.
- **Constraints:** Do not change code. This ticket only finds out the facts. The list below comes from the evidence reports. Treat each item as unproven until you read the code.
- **Acceptance:** For each item, write a section with: the status (real, not real, or unknown); the evidence (file and line); a proposal; whether the proposal changes the wire; the risk and the effort.
  1. The keymap `version` reuses the render version. Draft keymaps use version 0.
  2. The server sends zero or one edit in `view/render`. The rules for several edits are not defined.
  3. Draft keymaps that use a `runId` as a view id may be dead after the real-file cutover.
  4. The rule "one unacked version for each view" is not written. BUG-READ-1 mentions several unacked versions.
  5. There is no protocol version check. A new client with an old server, or the reverse, gets no readable error.
  6. The `keyModifiers` preference is read and logged. It does not change behavior.
  7. VS Code cannot replay a `passThrough` key that is not a printable character or a clipboard shortcut.
  8. A static client learns about a mode change only through context notifications. Check that this works for entry through invoke, key, menu and `setMode`.
  9. Positional requests use the last acked tree. Before the first ack, they use the current tree. The spec does not say this.
  10. `TASKS.md` section 3.6 says a prompt is a request. The code sends a notification, and the client answers with `view/promptRespond`.
  11. VS Code declares layouts that it does not honor (T004 fixes the declaration).
  12. Add each other wart that you find while you read the code.
- **Verify:** The audit file exists. Each item has a status and evidence. A reviewer confirms that no code changed.

### T113 rich-protocol-decisions (S)

- **Depends on:** T112. Use the stage 1 findings of T115, T116 and T117 when they exist. A later finding changes the record only through an additive change.
- **Target Files:** `docs/design/rich-protocol-v1.md` (new), `TASKS.md` sections 3.6, 3.9 and 4.
- **Reference Files:** `docs/design/rich-protocol-audit.md`.
- **Constraints:** Decide as little as possible. Choose the simplest rule that works for the clients that exist now.
- **Acceptance:** The decision record states these decisions:
  1. Core and optional parts. Core (frozen): the capability block; `view/open` and `view/close`; the three render forms; ack and resync; key and invoke; keymap; the basic effects; prompts; progress. Optional (declared): `editDocument`, `visitFile`, `clipboard`, picks, the structured menu model, text mode, and layouts other than `main`.
  2. The evolution policy. Inside v1, changes are additive only. A new effect or prompt kind needs a declaration. A breaking change raises `v`.
  3. The version check. The server reports its protocol version in the initialize result. A client reports the minimum version that it needs. Each side shows a readable message when the versions do not match.
  4. A decision for each real wart in T112: fix it, document it, or reject the proposal with a reason.
  5. The prompt transport stays as a notification plus `view/promptRespond`. Write it in section 3.6.
  6. The VS Code menu default (QuickPick or text document), and the mirroring of VS Code fold shortcuts when the user changes `editor.toggleFold`.
- **Verify:** Review the decision record. Each decision has a reason.

### T114 rich-protocol-cleanup (M)

- **Depends on:** T112, T113, T004.
- **Target Files:** `src/protocol.rs`, `src/session.rs`, `src/server.rs`, `src/keymap.rs`, `lua/magit-lsp/*.lua`, `clients/vscode/lib/*.js`, `tests/keys_protocol.rs`, `tests/plain_core.rs`, `tests/data/client-contracts.json`.
- **Reference Files:** `docs/design/rich-protocol-v1.md`.
- **Constraints:** Compatibility with old clients is not a goal. Make the wire changes now, before v1 is stable. Apply only the decisions in T113.
- **Acceptance:** Apply each "fix" decision on the server and on both clients. Delete each dead field and dead code path that T112 confirmed. Add the protocol version check on the server and on both clients. Update the shared test vectors. Each decision has a test that fails without the change. Update `docs/CLIENT_SPEC.md` for each wire change.
- **Verify:** Run `cargo test`, `make test-lua` and `node clients/vscode/test/test.js`. Run `scripts/ui run` on the status and menu scenarios.

### T105 vscode-f5-cause (S)

- **Depends on:** T001. This ticket moved here from phase 14.
- **Target Files:** `docs/evidence/vscode-host-behaviours.md`, `clients/vscode/lib/*.js`.
- **Context:** In VS Code, the F5 key did not reach the magit view in a test. The cause is not proven.
- **Acceptance:** Find the cause. Write it in the evidence file with the Codium version. Fix it, or document the limit and remove F5 from the generated bindings.
- **Verify:** Run the test that presses F5 in the status view. It must show the refresh, or the binding must not exist.

### T119 min-git-version (S)

- **Depends on:** none. This ticket moved here from T111.
- **Target Files:** `.github/workflows/*`, `README.md`, `docs/CLIENT_SPEC.md`, `tests/errors.rs`, `tests/git_discovery.rs`.
- **Acceptance:** Run the IMM-1 tests and the discovery tests on the oldest Git version that you want to support. Decide the minimum version. Write it in the README. Add a CI job for that version. The recommended version, 2.30.8, stays a recommendation unless you decide a hard minimum.
- **Verify:** The CI job passes on the minimum version.

### T115 visual-studio-client (L)

- **Depends on:** T113.
- **Target Files:** `clients/visual-studio/` (new), `docs/evidence/visual-studio-spike.md` (new).
- **Reference Files:** `docs/CLIENT_SPEC.md`; the Neovim and VS Code clients.
- **Constraints:** The client has no Git logic and no layout logic. Declare only the capabilities that the client implements. If a feature is not possible, use the Plain fallback. A request for a protocol change goes through T113. Check the extension model, the toolchain and the license of the IDE before you start.
- **Acceptance:**
  1. Stage 1, a short spike. Answer in the evidence file: Can an extension start the server and use LSP? Can it show a virtual document with server text? Can it intercept keys in that document? Can it show prompts and pickers? Can it open a real file at a location? Which capability block can it declare? Which parts of the protocol did you need that the spec does not give? Which tier works: Plain, Plain keys or rich?
  2. Stage 2, a prototype of the best tier that works. For rich: the status view, stage and unstage, a menu with an argument, and a commit edit with confirm and abort.
  3. List each change that the protocol needs, and send it to T113 as a request.
- **Verify:** The evidence file exists. The prototype builds. It passes the conformance scenarios that match its declared capabilities (T029).

### T116 zed-client (L)

- **Depends on:** T113.
- **Target Files:** `clients/zed/` (new), `docs/evidence/zed-spike.md` (new).
- **Reference Files:** as T115.
- **Constraints:** As T115. VERIFY what the Zed extension API allows for views, key bindings and prompts. It may allow only a language-server configuration. If so, the result is a Plain client and a finding.
- **Acceptance:** As T115 stages 1 to 3, for Zed.
- **Verify:** As T115.

### T117 jetbrains-client (L)

- **Depends on:** T113.
- **Target Files:** `clients/jetbrains/` (new), `docs/evidence/jetbrains-spike.md` (new).
- **Reference Files:** as T115.
- **Constraints:** As T115. VERIFY which JetBrains products offer an LSP API, and from which version. Test on the product that most users have.
- **Acceptance:** As T115 stages 1 to 3, for JetBrains.
- **Verify:** As T115.

### T118 rich-protocol-freeze (S)

- **Depends on:** T114, and stage 2 of at least two of T115, T116 and T117.
- **Target Files:** `docs/design/rich-protocol-v1.md`, `docs/CLIENT_SPEC.md` (level 4), `docs/schema/`.
- **Acceptance:** Review the requests from the prototypes. Apply the additive ones. Decide the others. Mark v1 as stable. Write the evolution policy in level 4 of the spec. Make sure that each prototype passes the conformance scenarios for its declared capabilities. Record which client anchors each optional capability (see rule 6). If no client uses an optional part, delete it (rule 5).
- **Verify:** Run the schema drift check and the conformance set on each client.

## Phase 7: Test stabilization (runs in parallel; does not gate 0.2)

### T023 capability-lattice (M)

- **Depends on:** T006, T008.
- **Target Files:** `tests/capability_lattice.rs` (new), `tests/common/editor_model/`, `tests/ui/plain-presets/` (new, confirm).
- **Constraints:** Neovim applies some features that it did not declare. Therefore the strict fake editor must prove that the server never sends an undeclared feature. Neovim cannot prove it.
- **Acceptance:**
  1. Add a lattice test. Take each conformance scenario with its requirement set R (from the recording `Ui`) and any capability set C. If C meets R directly or through a fallback, the final repository state must equal the state from a run with all capabilities. If C does not meet R, nothing changes, the client gets a readable message, and the document stays consistent.
  2. Always run the recorded real-trace presets. Add a pairwise covering set with constraints. Add a short hand-written list of interactions: position encoding with edit ranges; `documentChanges` with echo order; `showDocument` with the template refresh; mixed-rung chains; progress with cancel. Record the number of combinations and scenarios.
  3. Use a child Neovim over RPC (no tmux, no magit plugin). Start it with `vim.lsp.start{capabilities = ...}`. Run these presets: minimal (`applyEdit` and `codeAction`); plus `showDocument`; plus completion; plus progress; and full.
  4. Generate a table "what you get for what you declare" for T013.
- **Verify:** Run `make test-rust FILTER="--test capability_lattice"`.

### T024 host-spike (S)

- **Depends on:** T001.
- **Purpose:** Decide if a real extension host under a private Xvfb can replace the fakes `clients/vscode/test/fake/vscode.js`, `languageclient.js` and `integration.js` (about 820 lines of fake code, plus the integration test).
- **Acceptance:**
  1. Run one launch with the real language client and the real server, running a list of cases. Measure launch time, memory, and flake rate over 10 runs.
  2. Record these facts as text in `docs/evidence/vscode-host-behaviours.md`. Write the Codium version for each fact.
     - Does CDP `Input.dispatchKeyEvent` trigger contributed keybindings, including chords?
     - Can you see the native Save, Don't Save and Cancel dialogs as DOM text when `window.dialogStyle` is `custom`? Can you click them by role?
     - Does the clipboard stay inside the virtual display?
     - Does closing a tab emit `onDidCloseTextDocument`? Do tab objects keep their identity between reads? What does `setTextDocumentLanguage` fire? Does `showQuickPick` return `undefined` when the user dismisses it? Do `uriConverters` apply to every URI?
     - Do `ctrl+t` and `ctrl+shift+x` collide with workbench shortcuts?
     - Does a top-level `"//generated"` key survive `vsce package`?
     - Which `clientInfo` does the client send?
  3. Write a one-page decision. Delete the fakes (preferred), or keep a minimal modular stub.
- **Verify:** The evidence file exists and contains only text.

### T025 e2e-runner (M)

- **Depends on:** T024.
- **Target Files:** `clients/vscode/test/e2e/run.mjs`, `real-file-lifecycle.mjs`, `real-file-spike.mjs` (confirm), a new Xvfb wrapper script.
- **Context:** CI uses a qwerty layout. The desktop of the owner uses Colemak. The current runner passes `DISPLAY`, `DBUS_SESSION_BUS_ADDRESS` and `XDG_RUNTIME_DIR` through. It sets `keyboard.dispatch: keyCode` to compensate for the layout. A spike calls `bspc` on the real window manager.
- **Acceptance:**
  1. Start a private Xvfb on a free display for each run. Run `setxkbmap -display :N us` each time. Remove `keyboard.dispatch: keyCode`.
  2. Make the environment hermetic. Force `DISPLAY`. Remove `DBUS_SESSION_BUS_ADDRESS`, `WAYLAND_DISPLAY` and the real `XDG_RUNTIME_DIR`. Use `--ozone-platform=x11`. Remove each `bspc` call. Put the profile and extension directories in `/tmp` and delete them after each run. Do not use Docker.
  3. Send keys with CDP. Use XTEST only as a fallback, on the private display. Never run `xdotool` against the real display. Add a helper `press(binding)`. It converts a VS Code key string into `key`, `code` and `windowsVirtualKeyCode`. Use a table of about 40 US-layout keys.
  4. Run under `systemd-run --user --scope` with a low CPU weight. Keep the process census. Zero processes may stay after the run.
  5. Read dialogs as DOM text. Click them by role.
  6. The native tier is optional on your machine (`MAGIT_NATIVE=1`). CI always runs it.
  7. Add one line to the README. Say that bindings such as `shift+/` assume a US-like layout.
- **Verify:** On a machine with a non-US desktop layout, run `MAGIT_NATIVE=1 npm --prefix clients/vscode run test:e2e`. Record the output of `setxkbmap -query` for the private display.

### T026 host-cases (L)

- **Depends on:** T024, T025.
- **Target Files:** `clients/vscode/lib/hooks.js` and `debug.js` (new), `clients/vscode/test/e2e/cases/*.js` and `lib/*.js` (new), `suite.js`, `integration.js`, `clients/vscode/test/fake/*`.
- **Acceptance:**
  1. Add test seams. `hooks.js` and `debug.js` mirror `hooks.lua` and `debug.lua`. `snapshot()` returns: the active view, the view stack, the render version and acked version, the keymap scope, capture and pending state, the drafts, recent messages, and a tab and group dump. `idle()` is true when the last render is acked and no request is open. Do not use sleeps.
  2. Split `suite.js` (about 1,800 lines with an environment-flag ladder). Step 1 is mechanical. Move the helpers to `e2e/lib/`. Move each section to `e2e/cases/<name>.js`. Use the signature that `head-operations.js` already uses: `run({extension, evidence, repo, cdp, waitFor, checkpoint, ...})`. Make `suite.js` about 100 lines. It selects cases with `MAGIT_E2E_CASES`. Compare the evidence JSON before and after the split. Behavior must not change.
  3. One launch runs the smoke set. The smoke set: status render and fold with a real keybinding, including a chord; cursor and selection stage and unstage, including a selection that ends at column 0; log push and pop; menu `-a`, commit preflight confirm, editor open and explicit confirm; last-tab close, and abort.
  4. The on-demand risk set: dirty-close dialogs, multi-group tabs and lifecycle.
  5. Move feature acceptance (`head-operations`, most A34 variants, the theme case) to Rust or Node tests, or delete it.
  6. Port what the fakes still cover. Then delete `fake/vscode.js`, `fake/languageclient.js` and `integration.js`. If T024 decided to keep a minimal stub, follow that decision.
  7. Add a loop over each declared effect, prompt kind and layout. Add a loop that presses each contributed binding in its scope. Add goldens from the T002 recorder. They must be small and normalized, with semantic assertions.
- **Verify:** Run `MAGIT_NATIVE=1 npm --prefix clients/vscode run test:e2e` and `make vscode-test`.

### T027 keybinding-compaction (M)

- **Depends on:** none.
- **Target Files:** `clients/vscode/scripts/gen-keybindings.mjs`, the keymap generator in `src/` (confirm), `clients/vscode/package.json`, `.gitattributes`, `Makefile`.
- **Acceptance:**
  1. First check these facts. About 28 entries use `magit.scope == 'common'`, and no view reports that scope. The `commit-message` scope block, and the `resourceScheme == magit-commit` commands and menus, are dead. Each pair of `ctrl+t` entries differs only in `magit.textMode`.
  2. Write `common` bindings once. Guard them only with the focus and widget terms. Write the differences of each scope after them. Add an explicit negation only where a scope removes a common binding. Merge each `ctrl+t` pair.
  3. Delete the dead blocks that step 1 confirmed. Delete the `lib/edit.js` references to them.
  4. Mark the generated block as generated (see rule 4). Check for drift in `make vscode-test`.
  5. Add a denylist test for the generated bindings. The test must fail if `ctrl+p`, `ctrl+shift+p`, `ctrl+w`, `ctrl+tab`, `f1` or `alt+tab` is bound. List each exception with its reason and its `when` clause.
  6. If the Rust generator already makes Neovim bindings, check if `lua/magit-lsp/keys.lua` can be smaller.
- **Verify:** Run `make vscode-test` and `node clients/vscode/test/test.js`. Record the number of generated entries before and after.

### T028 colour-roles (S)

- **Depends on:** T001.
- **Target Files:** `src/semantic_tokens.rs`, `lua/magit-lsp/highlights.lua`, `clients/vscode/package.json`, `clients/vscode/lib/semantic.js`, `scripts/color_spike.lua`, `Makefile`, `ui-test-plan.md`.
- **Constraints:** Semantic tokens carry names, not colors. Test only that the token names are correct. Do not test contrast, multiple color schemes, or theme changes.
- **Acceptance:**
  1. Delete `scripts/color_spike.lua`, `make color-spike` and each HTML output. The spike fails at setup, and `tohtml` cannot see LSP tokens.
  2. Reduce the 28 token types to about 10 roles: heading, muted, added, removed, changed, path, hash or identifier, ref, key or accent, warning or error. A modifier only adds an attribute. Each emitted type names its role. A test fails when a role is missing. Use a standard LSP token type when one fits.
  3. Derive the scopes of VS Code from the role table. VS Code now contributes 5 of 28 types and ignores `tokenTypeGroups`.
  4. Add three tests. First, a Rust test of the token response for a fixture view. Second, a data test for each client: each legend type has a Neovim link or a VS Code scope. Third, one ANSI smoke test for each client with one color scheme. It must show that added and removed cells have different foreground colors. Use `decoder.py` and `expect style`.
- **Verify:** Run the Rust token tests and `make test-lua`. Run one `.scn` color smoke test.

### T029 scenario-slimming (M)

- **Depends on:** T006, T020.
- **Target Files:** `scripts/ui_lib/*`, `tests/ui/scenarios/*.scn`, `ui-test-plan.md`, `tests/conformance/` (new, confirm).
- **Acceptance:**
  1. A scenario runs one combination by default: 80x24 and one keymap profile. `matrix: true` turns on all sizes and profiles. Write `.screen` and `.ansi` files only on failure or on request. Existing scenarios keep working.
  2. Add a conformance set of 6 to 8 scenarios as data: open status; stage a file; stage a hunk; stage one visual-selected line and then unstage it without a visual selection; fold; a menu with an argument; a commit with confirm and with abort (assert the HEAD message); log select confirm and cancel. Use these verbs: `open`, `moveToText`, `select`, `invoke`, `respondPrompt`, `saveFile`, `closeDocument`, `confirm`, `abort`, `expectDocument`, `expectRepo`. Find targets by text. Never use line numbers.
  3. Each driver has a thin interpreter in its own language: Rust for the fake editor, Python for `ui_lib`, Node for the VS Code host.
  4. `invoke` is derived. A rich driver presses the binding from the manifest. An unbound command must fail. The Plain driver runs the code action with the command id. It never uses the title.
  5. Derive `requires:` from the recording `Ui`. Driver-only properties are `closeSignal`, `cursorControl` and `nativeDialogs`.
  6. Run the same scenario on the fake editor and on real plain Neovim. A difference is a bug in the fake.
  7. Report a matrix of scenario by driver. Show each skip with its reason. Make sure that each feature has at least one driver that did not skip.
- **Verify:** Run `make ui-unit`. Run `scripts/ui run` on the conformance set with `client: rich` and with `client: plain`.

## Phase 8: Release (starts after phases 1 to 6)

Do not schedule other feature work between the gates and the release. A ticket stays open until its evidence exists.

### T030 release-artifacts (M)

- **Depends on:** T022, T013. Old ticket: R-REL-1 (parts 1a and 1b are done).
- **Target Files:** `.github/workflows/*`, `scripts/package-release.sh`, `scripts/install.sh`, `Cargo.toml`, `clients/vscode/package.json`, `docs/RELEASING.md`, license files.
- **Reference Files:** `docs/RELEASING.md` (Linux x86\_64 GNU artifact CI, version alignment, installer, license split).
- **Constraints:** Never publish with the credentials of an unprotected pull request. State which platforms are tested and which are build-only. Never call an unsupported platform tested. VS Code installation must not need Rust. Document if the server is bundled, downloaded or external.
- **Acceptance:** Do dry runs for crates.io, the VS Code Marketplace and Open VSX. Review the dependencies, the upstream licenses and the corresponding source. Install and start the real binary, crate and VSIX on a clean machine. Record one hosted CI run. Make checksums. Document how to rerun safely and how to recover from a partial publication. Record the approval to publish.
- **Verify:** Do dry runs and clean-machine starts on each claimed target. Review the CI workflows.

### T031 client-install-docs (S)

- **Depends on:** T030. Old ticket: R-REL-2 (part 2a is done).
- **Target Files:** `docs/CLIENT_INSTALL.md`, `README.md`.
- **Constraints:** README screenshots are a manual task for a human (`scripts/readme-shots/`). Do not add a CI job for images. Welcome Helix, Zed and JetBrains as community clients. Do not promise first-party support.
- **Acceptance:** Smoke-test the lazy.nvim manager path and the install of the published VSIX. Link the client spec. State the status of each manual asset.
- **Verify:** Run clean setup smoke tests for Neovim and VS Code. Check the links.

### T032 release-readme (M)

- **Depends on:** T030, T031, T013. Old ticket: R-REL-3.
- **Target Files:** `README.md`.
- **Acceptance:** Show features against Planned items. State the purpose and the difference from other tools. Make only modest claims about speed, and only with evidence. Describe both clients, the install and the quickstart. Add a short, fair comparison with Neogit and Edamagit. Research it when you write it. Evaluate these candidate advantages, and do not assert them: key-handling speed, a scriptable command interface, and preselection of the commit at the cursor. The comparison may conclude that no strong Neovim-specific advantage exists. Add badges for GitHub Releases, crates.io, the VS Code Marketplace and Open VSX. Check the badge targets before the tag. Add coverage badges with a clear scope. Link the client spec. At the end, add the disclosure of the author about how the code was made and tested. Do not say that tests replace human review.
- **Verify:** Render the README. Check each link, asset, badge source and product claim.

### T033 release-go-no-go (M)

- **Depends on:** T030, T031, T032, and every gate. Old ticket: R-REL-4.
- **Acceptance:** Write a short changelog. Add warnings about history rewrites. State the known limits and the platform matrix. Disclose the remaining limits of BUG-READ-1 for the same session. Record a full `make test`, the phase evidence, and a `make hygiene` run. Do the install and publication dry runs. Record an explicit human go or no-go before the tag. After approval, the tagged CI publishes all channels. It then checks the publication and the live badges. Windows and macOS acceptance, other menus and views, and build-only targets are not gates.
- **Verify:** Run `make test`. Collect the native evidence and the clean-install smoke tests. A human reviews the changelog, the warnings and the limits.

## Phase 9: Plain keys (optional; starts after T022)

Plain keys is an optional input path for editors that cannot bind code actions to single keys. The user types one character. The server reads it as the matching Magit key and renders again. The render removes the character. Plain keys never runs a destructive command. Code actions are the only path for destructive commands.

The settings have these layers. The most specific layer wins. Layer 1: built-in default (off). Layer 2: the user setting `plain.keys.default`. Layer 3: a section for each client (by `clientInfo.name`). Layer 4: the toggle for each document. A limit from the quirks table is above all layers. The header row shows the effective state and its source. There is no setting for each repository.

### T034 key-edit-shapes (S, gate)

- **Depends on:** T002. Old ticket: PK-1 (the Plain keys task file).
- **Target Files:** `tests/fixtures/lsp_traces/plain_keys/`, `docs/evidence/plain-keys-edit-shapes.md`, `tests/common/editor_model/` (new profiles).
- **Reference Files:** the recorded Helix `didChange`. It is the only captured shape so far: a single insert at 0:0.
- **Constraints:** Record only. Do not write production code. Mark a shape "reconstructed" if you did not capture it.
- **Acceptance:** Capture raw `didChange` traces with versions from plain Neovim and from Helix for these actions: an insert of one character; a selection replace (by character and by line); a paste of one character; a paste of several characters; rapid typing; an undo or redo right after a server repair edit. The delete-then-insert pair is the base of the design. Capture it, or mark it "reconstructed". Add these profiles: `selection-replace-pair`, `selection-replace-single`, `rangeless-full-text`, `batched-changes`, `paste-multichar`, `undo-after-repair`. In the evidence file, state which editor makes which shape. Flag each shape that is not one of the three accepted patterns. The patterns: a single insert; one ranged replace by one character; a delete followed by a one-character insert at the same start in the next notification.
- **Verify:** Run `make test-rust FILTER="--test plain_harness"`. Review the evidence before you start T035.

### T035 key-classifier (M)

- **Depends on:** T034.
- **Target Files:** `src/docstore.rs`, `src/presentation/keys.rs` (new), `tests/plain_keys_input.rs` (new).
- **Constraints:** Use pure functions. Do not use Git or I/O. Use the negotiated position encoding. Find echoes of server edits by the resulting text hash. Never use a flag.
- **Acceptance:** Make `NetEdit{pre_hash, version_before, version_after, pre_range, inserted}` for each `didChange`. Compose several ranged changes in order. Compare rangeless changes by common prefix and suffix. Otherwise mark the edit `Complex`. A classifier has the states Ready, PendingDeletion and Executing. It returns `Accept`, `Wait` or `Reject(repair)`. It accepts only the three patterns. It uses the snapshot whose hash equals the text before the edit. A deletion alone waits. It expires to `Reject(repair)` after `plain.keys.pair_timeout_ms` (default 250). It never pairs with a later insert that is not related. Newline, tab, many characters, paste, empty range and header edits are `Reject`. Map the space key to `Space`, `+` to `Plus`, and other printable keys to themselves. Convert a half-open range to inclusive lines. An end at column 0 means the previous line. Test with each fixture from T034, and with multibyte text, CRLF, newline-only ranges, and deletions that end at column 0.
- **Verify:** Run `make test-rust FILTER="--test plain_keys_input"`.

### T036 key-policy-settings (M)

- **Depends on:** T034, T005, T008.
- **Target Files:** `src/commands.rs`, `src/presentation/plain.rs`, `src/config.rs`, `docs/plain-support.md`, `tests/plain_keys_policy.rs` (new), `tests/plain_keys_arm.rs` (new).
- **Acceptance:** Add a `destructive: bool` attribute to each command (default false). Set it for each command that discards, hard-resets, deletes or force-pushes. A CI test runs each command against the recording `Ui`. It fails if a command sends a destructive confirm without the flag. A command can take character input if it is `Implemented`, not destructive, and bound in the active profile. The keys state of each document is off by default. A code action "Keys: on/off" toggles it. The server renders a header row that shows the effective state and its source. Repair each user edit to that row. The state stays through view-stack changes and resets when the document closes. A prompt view and the commit file never arm. Implement the setting layers above, with the quirks limit on top.
- **Verify:** Run `make test-rust FILTER="--test plain_keys_policy --test plain_keys_arm --test manifest --test parity"`.

### T037 key-execution (M)

- **Depends on:** T035, T036, T022.
- **Target Files:** `src/presentation/plain.rs`, `src/presentation/keys.rs`, `src/session.rs`, `tests/plain_keys_exec.rs` (new).
- **Constraints:** Reuse the existing key handler and profile keymap. Do not add a second keymap. Resolve the key against the tree of the matched snapshot. Never read text. Do not send keymap pushes to Plain clients. While a command runs or an edit applies, drop other edits and repair them after. Do not queue them.
- **Acceptance:** After a command, a repair render restores the authoritative text. This removes the typed character. Compare it with the current text in the document store. Chord and menu steps work one character at a time, with the `pending` state of the view shown in the document. A key that is not eligible, is destructive, is `Planned` or is not bound gives a one-line message and a repair. A destructive key points to the code action. A stale target gives the message "changed since displayed; refreshed" and changes nothing. Scenarios: stage a file; stage a hunk; stage a visual selection; unstage; `n` and `p`; `c` opens the commit menu and `-a` toggles. Assert the exact Git state in each scenario.
- **Verify:** Run `make test-rust FILTER="--test plain_keys_exec"`.

### T038 key-safety-e2e (M)

- **Depends on:** T037.
- **Target Files:** `tests/plain_keys_safety.rs` (new), `tests/plain_keys_e2e.rs` (new), `tests/ui/scenarios/plain-keys-*.scn` (new), `docs/evidence/plain-keys-helix.md` (new).
- **Acceptance:** Write a property test. It makes random edit sequences (inserts, deletes, pastes, undo-like re-inserts, mixed with renders and echoes) on a real disposable repository. Git state changes only for an armed, eligible candidate with the correct shape against a matching snapshot. After any divergence, the document equals the authoritative text. Add these targeted tests: a fast repeated key; a server echo never runs; a paste and a many-character input are rejected; edits in another URI are ignored; edits in a prompt field are field input; edits in the commit file are never read as keys; keys off ignores all edits; an old pending deletion never joins a later insert; an undo after a repair is repaired (document the known re-insert risk). Add plain-Neovim scenarios: a single-key stage in insert mode; a stage by visual-selection replace; typing with keys off is repaired; a destructive key is refused. Add a Helix manual checklist: selection-replace pairing; restore after the timeout for a lone deletion; keys on and off.
- **Verify:** Run `make test-rust FILTER="--test plain_keys_safety --test plain_keys_e2e"` and `scripts/ui run tests/ui/scenarios/plain-keys-*.scn`.

### T039 plain-keys-docs (S)

- **Depends on:** T038.
- **Acceptance:** Add a Plain keys section to the client spec. State what a client must send, the accepted edit patterns, the key set and its exclusions, arming, the timeout, the undo risk, and that it never works in prompts or in the commit file. Describe the setting layers. In the README, say that Plain keys is optional, and state its limits.
- **Verify:** Review the documents against the fixtures.

## Phases 10 to 13: Post-release tickets

These tickets do not gate 0.2. Each line is a ticket in short form. Expand a line into the full template when you schedule it. Keep the Verify lines from the current `TASKS.md`. For user-visible work, use focused scenarios.

### Phase 10: Small features

| ID | Name | Size | Depends on | Task |
| --- | --- | --- | --- | --- |
| T040 | commit-signing | M | none | Add `--gpg-sign`. Handle pinentry and errors. |
| T041 | log-variants | S | none | Add log variants and filters to the existing log. |
| T042 | remote-config-tags | S | none | Add remote configuration and lightweight tags. |
| T043 | branch-basics | S | none | Add branch configure, create, rename and delete. |
| T044 | notes-ignore | S | none | Add notes and ignore. |
| T045 | theming-docs | S | T028 | Document theme customization after the colour roles are fixed. |
| T046 | forking-docs | S | none | Write `docs/FORKING.md`. |
| T047 | config-location-edit | S | none | Use the `dirs` crate for the config directory on macOS. Add a root command in each client that opens the config file for editing. |

### Phase 11: Read views

| ID | Name | Size | Depends on | Task |
| --- | --- | --- | --- | --- |
| T048 | view-data | I | none | Make server-owned structured data for revisions and diffs. |
| T049 | revision-view | M | T048 | Add a revision view. In a plain log, Enter opens the commit. In select mode, Enter selects. |
| T050 | diff-view | M | T049 | Add a diff view with ranges and arguments. `q` returns to the log. |
| T051 | picker-inspect | M | T049 | Let the user inspect a commit from a picker and return to it. The keys `o` and `q` are proposals. They are not approved. |
| T052 | refs-view | M | T048 | Add a refs view. |
| T053 | file-at-revision | M | T048 | Add read-only documents for a file at a revision. |
| T054 | reflog-view | M | T052 | Add a reflog view. |
| T055 | line-history-view | M | T052 | Add a line-history view. |
| T056 | log-links-hover | S | none | Add `documentLink` and `hover` for hashes and refs. |
| T057 | log-semantic-styling | M | T028 | Add server-owned styling for log elements, with the colour roles. |

### Phase 12: Shared Git workflows

| ID | Name | Size | Depends on | Task |
| --- | --- | --- | --- | --- |
| T058 | diff-arguments | M | none | Pass diff arguments to all views. Read the context size from config. Do not hard-code `-U3`. |
| T059 | push-baseline | I | none | Find the push target, or else the upstream. Never fetch. Show when no baseline exists. |
| T060 | status-history-sections | I | T059, T048, T049 | Add the Unpushed section (newest 20 commits open, older commits in a lazy `More` group) and the Pushed section (closed in new views, lazy, with a limit). User fold choices survive a refresh. With no baseline, a detached HEAD or an unborn repository, show a labeled fallback. |
| T061 | unpulled-and-headers | M | T059 | Add the Unpulled section and the Head, Merge, Push and Tag header lines. |
| T062 | commit-staged-diff | M | none | Show the staged diff next to the commit message. |
| T063 | commit-trailers-lint | M | T062 | Add trailers and lint diagnostics. |
| T064 | commit-edit-close | S | T062, T063 | Make confirm, cancel and close work with the companion view. |
| T065 | targeted-fixup-squash | M | T014 | Add targeted fixup, squash, augment and alter. |
| T066 | instant-rewrite | M | T065, T019 | Add instant fixup and squash rewrites. |
| T067 | branch-checkout-spin | M | T043 | Add checkout, create-and-checkout, spin-off and spin-out. Report unsafe operations. |
| T068 | network-seam | I | none | Add a seam for progress, askpass and cancel in network operations. |
| T069 | fetch-push-pull | M | T068; pull also T017 | Add fetch and push flags. Add pull `--rebase` and `--ff-only`. Handle conflicts. |
| T070 | branch-reset | M | none | Add branch reset. Keep it separate from file checkout at a revision. |
| T071 | annotated-tags | S | T042 | Add annotated and signed tags. |
| T072 | stash-views | M | T048 | Add stash views, stash capture, a Stashes section and a stable stash identity. |
| T073 | stash-operations | M | T072 | Add apply and pop with conflicts, branch and drop. |

### Phase 13: Advanced workflows

| ID | Name | Size | Depends on | Task |
| --- | --- | --- | --- | --- |
| T074 | merge-conflicts | M | T017 | Add the merge menu, the Unmerged section, conflict actions, continue and abort. |
| T075 | mergetool-effect | M | T074 | Add an effect that opens the native merge tool. |
| T076 | sequencer-state | M | T017 | Add sequencer state for cherry-pick and revert. |
| T077 | cherry-view | M | T076 | Add a read-only Cherry view. |
| T078 | rebase-options | M | T017 | Add linear rebase options: upstream, pushRemote or elsewhere, autosquash and update-refs. |
| T079 | rebase-merges | L | T078, T018 | Add the full `--rebase-merges` todo grammar and the graph rewrite. |
| T080 | scripted-sequence-edit | M | T078 | Add scripted `GIT_SEQUENCE_EDITOR` edits. A reword can still use the editor bridge. |
| T081 | picker-integrations | M | T014 | Add the other commit-picker workflows on the shared selection code. |
| T082 | blame-view | M | T048 | Add a blame view. |
| T083 | blame-inline | M | T082 | Add inlay hints and code lens in real files. |
| T084 | run-command | M | none | Add a checked run-command workflow. |
| T085 | patch-workflows | M | none | Add safe apply and format-patch workflows. |
| T086 | am-state | M | T017 | Add the am-state workflow. |
| T087 | bisect-state | M | T017 | Add the bisect-state workflow. |
| T088 | multi-repo-identity | L | none | Add bounded identity for repository sessions and worktrees. |
| T089 | worktree-operations | M | T088 | Add worktree operations. |
| T090 | submodule-workflows | M | none | Add submodule workflows. |
| T091 | buffer-repo-context | M | T088 | Find the repository and path for an editor buffer. |
| T092 | any-buffer-dispatch | M | T091, T082 | Add file log, diff, stage and blame commands for any buffer. |
| T093 | section-layout-policy | M | none | Make section layout a server policy. |
| T094 | full-line-tint | M | T093 | Let clients show a full-line background tint from server data. The owner wants this. |
| T095 | profiling-budgets | M | none | Measure and document budgets and size limits. |
| T096 | incremental-render | M | T095 | Add node-level incremental rendering only if profiling shows a need. |
| T097 | windows-paths | M | none | Check Windows paths and CRLF. This is not a release gate. |
| T098 | jj-fork | L | T046 | Make an optional jj fork. Do not weaken the Git boundary. |

## Phase 14: Deferred (not scheduled)

Do not start these tickets until the owner asks.

| ID | Name | Depends on | Task |
| --- | --- | --- | --- |
| T099 | edit-history | T010 | Add history prev and next as a server code action. The code action sends a versioned `applyEdit`. It works the same for each client. |
| T100 | windows-editor-bridge | none | Study the Windows named pipe, ACL and Job-object design. Document the limits. Make no claim without evidence. |
| T101 | macos-editor-bridge | none | Run the process-group and reaping tests on a macOS CI runner. Until then, macOS is not verified. |
| T102 | cross-hunk-selection-audit | none | Find out if a selection across hunks acts on lines or on whole hunks. Fix only a bug that you can show. |
| T103 | same-session-read-redesign | none | Keep a published state for each view. Handle acks directly. Queue resize events. Ask the owner before you start if the work needs a rewrite of `View` or `CommandCtx`. |
| T104 | editor-lifecycle-residuals | none | Check these cases for the real-file editor: exit from the last window, shutdown with a dirty buffer, builds of other versions, and wider reload and workspace changes. |
| T105 | vscode-f5-cause | none | Find why VS Code intercepts F5 in the magit view. The cause is not proven. |
| T106 | pick-delivery-modes | T008 | Add paged picks (the first page and `hasMore`) and query picks (the client sends the typed text, the server returns ranked matches). Build them only when a command needs a file or commit-search picker. |
| T107 | skip-at-point-option | T008 | Add an opt-in setting (default off) that skips the picker when the cursor is on a commit or ref. It can ask for a confirm. |
| T108 | server-fuzzy-picker | T106 | Add a server-side fuzzy picker. It needs one round trip for each key press. |
| T109 | multi-client-repository | none | Study two clients on one repository: mutation order, watcher refresh to all clients, and stale targets across clients. Today this is not supported. |
| T110 | rich-protocol-form | T038 | Decide the form of the rich protocol after real use. The rich protocol stays, and it must be able to compete with native extensions. Measure latency, the capabilities that Plain plus a key shim lacks, and the code and test cost. Until the decision, any extension that has one real consumer stays. |
| T111 | open-design-questions | none | Decide these questions: the default VS Code menu (QuickPick or a text document); trust for the config file in each repository (`.magit-lsp.toml`); the minimum Git version (run the IMM-1 tests on old versions); use of LSP 3.18 `workspace/textDocumentContent`; mirroring of the VS Code fold shortcuts when the user changes `editor.toggleFold`. |

## Old numbers to new numbers

Use this list to find a ticket from the history. Each open ticket in the old `TASKS.md` and in the two planning documents has a new number, or a reason why it has none.

- **Phase 1 and 2:** H-01 is T001. H-02 is T002. H-18 is T003. H-03 is T004. H-04 is T005. H-05 is T006. H-14 is T007.
- **Phase 3:** R-PLAIN-PROMPT is T008, with H-15 and the option rows of R-PLAIN-STACK. R-PLAIN-SELECT is T009, with H-15. R-PLAIN-DRAFT is T010.
- **Phase 4:** H-11 is T011. H-12 is T012. H-13, R-PLAIN-SPEC and the new work after R-CLIENT-SPEC are T013.
- **Phase 5:** R-SELECT-OPS is T014. R-4.1a is T015. R-EDIT-KINDS is T016. R-OP-STATE is T017. R-TODO is T018. R-7.3a is T019.
- **Phase 6:** R-PLAIN-E2E is T020. R-PLAIN-HELIX is T021. R-PLAIN-GATE is T022.
- **Phase 7:** H-06 is T023. H-07 is T024. H-08 is T025. H-09 is T026. H-10 is T027. H-16 is T028. H-17 is T029.
- **Phase 8:** R-REL-1 is T030. R-REL-2 is T031. R-REL-3 is T032. R-REL-4 is T033.
- **Phase 9:** PK-1 to PK-6 are T034 to T039.
- **Phase 10:** R-4.1-signing is T040. R-5.3a is T041. R-6.3a is T042. R-6.1a is T043. R-8.2a is T044. R-9.1a is T045. R-9.5a is T046. The edit-config command from section 9 of `TASKS.md` is T047 (it had no ticket).
- **Phase 11:** R-VIEW-DATA is T048. R-5.2a is T049. R-5.2b is T050. R-5.2c is T051. R-5.3b is T052. R-5.3c is T053. R-5.3d is T054. R-5.3e is T055. R-5.4 is T056. R-5.6 is T057.
- **Phase 12:** R-5.5 is T058. R-BASELINE is T059. R-4.3 is T060. R-6.4 is T061. R-4.2a is T062. R-4.2b is T063. R-4.2c is T064. R-4.1c is T065. R-4.1d is T066. R-6.1b is T067. R-NETWORK is T068. R-6.2 is T069. R-6.3b is T070. R-6.3c is T071. R-6.5a is T072. R-6.5b is T073.
- **Phase 13:** R-7.1a is T074. R-7.1b is T075. R-7.2a is T076. R-7.2b is T077. R-7.3-options is T078. R-7.3b is T079. R-7.3c is T080. R-7.4 is T081. R-8.1a is T082. R-8.1b is T083. R-8.2b is T084. R-8.2c is T085. R-8.2d is T086. R-8.2e is T087. R-9.3a is T088. R-8.2f is T089. R-8.2g is T090. R-8.3a is T091. R-8.3b is T092. R-9.1b is T093. R-9.1c is T094. R-9.2a is T095. R-9.2b is T096. R-9.4 is T097. R-9.5b is T098.
- **Phase 14:** R-EDIT-HISTORY is T099. R-EDIT-7 is T100. R-EDIT-9 is T101. R-SELECT-AUDIT is T102. The larger design of BUG-READ-1 is T103. The R-EDIT-REAL residual cases are T104. The F5 cause is T105. The picker extensions are T106, T107 and T108. Multiple clients are T109. R-STRAT-RICH is T110. The open questions in section 9 are T111.
- **No new number:** H-00 (the baseline is green). R-4.1-basic (R-4.1-head-edit and `commit-menu-shows-actions.scn` cover it). R-PLAIN-ACTIONS, R-PLAIN-STACK, R-PLAIN-CORE and R-EDIT-REAL (closed umbrellas). The second entry of R-DIFF-SAFETY (done).
