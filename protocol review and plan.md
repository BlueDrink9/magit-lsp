# magit-lsp: harness, client-model and Plain-keys task list

*Prepared 1 November 2026 from the planning discussion. Companion to TASKS.md. Ticket bodies follow the roadmap template (Target Files, Reference Files, Constraints, Acceptance, Verify). Paths marked **(confirm)** were inferred from evidence reports and must be checked by the agent before editing. Items marked **VERIFY** are claims nobody has confirmed yet.*

## 0. How to use this list

- IDs: `H-nn` are harness/client-model tickets, `PK-n` are Plain-keys tickets, `D-n` are deferred stubs. Existing roadmap IDs (R-PLAIN-*, R-EDIT-*, R-REL-\*) keep their meaning.
- Sizes are relative (S/M/L), not time promises.
- Each ticket lists what it depends on. Tickets with no unmet dependency may run in parallel; merge conflicts are most likely in `src/server.rs`, `package.json` and `TASKS.md`.
- Do not mark a ticket complete without the evidence its Verify line requires. Never claim a green full gate without running it.
- When a ticket completes, collapse it per rule G6.

## 1. Ground rules (apply to every ticket)

- **G1. Text-only evidence.** No agent ever produces, opens, reads or mentions an image (PNG, JPEG, video, Playwright trace) as evidence or acceptance. Acceptable evidence: document text, tmux `.screen` text, ANSI text (`.ansi`), JSON state dumps, wire logs, extension-host logs, DOM text dumps over CDP, tab/group dumps, repo state (HEAD, index tree, worktree diff, message bytes), process census. Screenshot capture for the README is a human task whose code lives only in `scripts/readme-shots/` behind `make readme-shots`; agents must not run it or open its output. `clients/vscode/test/no-vision.js` keeps enforcing this for e2e sources.
- **G2. Test policy.** CI runs everything. Local full runs are ad hoc (native tier opt-in with `MAGIT_NATIVE=1`). Feature tickets require Rust state tests plus the conformance scenarios; Neovim L3 only when the visible behaviour is novel. VSCode real-host evidence is required only when a ticket changes a declared capability or files under `clients/vscode/`. Remove "native evidence in both clients" wording from feature tickets.
- **G3. Snapshots and goldens.** `UPDATE_EXPECT=1` and `cargo insta accept` are used rarely and judiciously. A change to a golden or snapshot goes in its own commit and the commit message explains every changed hunk. Goldens are small, normalized (ids, tokens, paths, timestamps stripped) and paired with semantic assertions that regeneration cannot change. Goldens are NOT marked `linguist-generated`, so diffs stay visible in review.
- **G4. Generated artifacts.** Generate whatever can be generated. Commit a generated file only if downstream clients need it as shipped (keybindings in `package.json`, `.d.ts`, spec tables, example files). Committed generated files carry a clear marker (header comment where the format allows; a top-level `"//generated"` key in `package.json`, VERIFY that VS Code and vsce ignore it), are listed in `.gitattributes` as `linguist-generated`, and have a CI drift check. Everything else is generated at build time and not committed.
- **G5. Delete, don't deprecate.** No shims, no compatibility. A `keep-for:` comment or `#[allow(dead_code)]` is not a licence to retain dead code. A declaration, option or variant stays if at least one real consumer uses it; otherwise delete it.
- **G6. Ticket size.** A completed ticket collapses to at most 12 lines: requirement, `Done: <commit>`, and a pointer to the test that now enforces it. Evidence narratives live in commit messages. One home per fact; other files link.
- **G7. Fakes.** A fake is trustworthy only if the same assertions run regularly against the real thing and expected values come from a recorded real run. Git is never faked. Every capability module in a fake carries an `anchor:` comment naming the real client or trace that exhibits it, or `anchor: none yet`. Anchors are comments, not enforced tables.
- **G8. Client-claim hygiene.** Absence of a client capability means "unsupported" only where the declaration is the feature itself (`window.showDocument`, `workspace.applyEdit`, `semanticTokens`, `workspaceEdit.documentChanges`). Where a capability object only describes sub-features (`definition`, `references`, `executeCommand`), absence is not a signal.
- **G9. `applied:true` is not proof.** Neovim returns `applied = true` from `workspace/applyEdit` even when it skipped a stale edit. Servers must reconcile by the resulting text hash, never by the response alone. One document per `applyEdit`.

## 2. Decisions record

**Testing**

- VSCode tests verify the adapter, not features. Feature behaviour is tested once in Rust and in the conformance scenarios.
- Prefer a real VS Code extension host under a private Xvfb over a hand-written `vscode` fake. Several seconds per launch is acceptable. Keep a minimal stub only if the measured cost is unacceptable.
- The fake LSP client is a minimal base plus capability modules, run in strict mode (undeclared use fails). Capability combinations are tested, not specific clients; real-trace presets always run.
- Neovim with a chosen `capabilities` table (over RPC, no tmux) replaces Helix as the range-of-richness test client. Helix is a source of recorded fixtures and an occasional manual smoke; never a CI dependency.
- Test named profiles `rich-native`, `rich-doc`, `plain`. Joint tests only for: mixed-rung command chains (cancel at each step), a refresh arriving during a pending prompt, commit editing with an active view stack, override-versus-declaration fallback.
- New conformance scenarios run at 80x24 in one keymap profile. Existing `.scn` corpus keeps working but defaults to a single combination; the 2x2 matrix becomes opt-in.
- Colour: test semantic correctness (Rust token response, per-client legend coverage, one ANSI smoke). Do not test theme behaviour, contrast or multi-scheme matrices.

**Pickers and prompts**

- Prompts are semantic; presentation is per client. The Plain mechanisms (document prompt, rows, code actions) are the universal fallback for any undeclared capability, even in a rich client, and are skipped when a better native widget is declared. Ladder: native pick, then server-rendered view, then code actions over a document.
- Commit or ref at point is always a preselection; the picker always appears. Magit's skip-at-point is an opt-in setting, default off, deferred.
- Rule: a target reachable from HEAD uses log select (fixup, squash, reword, rebase from HEAD). Everything else uses a text pick that accepts free text (anything `git rev-parse --verify` accepts) with the at-point value preselected. IMM-6 (stage/unstage/discard act on the smallest node under the cursor) is unaffected.
- Clients never filter, parse or resolve Git state. Native pickers over server-supplied stable ids comply; the blanket "no client picker" wording in §3.6/§3.7 is removed.
- `secret` and askpass are rich-only; Plain cancels cleanly with a message.
- Static pick delivery is enough for now. Paged and query delivery are deferred.

**Capability model**

- Resolution order: declared or derived capabilities, then quirk overrides, then user config. A user preference inside the declared ceiling wins; exceeding the ceiling needs an explicit per-client config key.
- Quirks live in one table mapping each quirk to a capability override, a server workaround and a fake-editor behaviour profile. Quirks match by `clientInfo` or by capability predicate. Cheap always-correct workarounds are applied universally and not gated.
- Gate Plain on `textDocument.codeAction` only, not `workspace.executeCommand`.
- Rich is kept. It must remain able to compete with native extensions; the open question (R-STRAT-RICH) is its form, not its existence.

**Plain keys** is a real track (PK-1..6). Settings layering, most specific wins: built-in default (off), user global `plain.keys.default`, per-client section keyed by `clientInfo.name`, per-document runtime toggle. No per-repository level. A quirks-table ceiling sits above all of them; the header row shows the effective state and its source.

**Documents**

- The client spec is a principal deliverable (H-13). Every JSON example is a real file validated against the schema and replayed against the real binary.

## 3. Dependency map

| Phase | Tickets | Needs |
| --- | --- | --- |
| A. Foundations | H-00 baseline, H-01 docs diet, H-02 wire recorder | none (H-02 after H-01 minimal) |
| B. Capability model | H-03 typed declaration and purge, H-04 quirks table, H-05 capability-model fake editor, H-06 lattice tests | H-03 after H-00; H-04 after H-02; H-05 after H-02..H-04; H-06 after H-05 and R-PLAIN-PROMPT |
| C. VSCode | H-07 host spike, H-08 e2e runner, H-09 host cases, H-10 keybinding compaction | H-07 after H-01; H-08 after H-07; H-09 after H-07, H-08; H-10 independent |
| D. Schema and spec | H-11 schema, H-12 example files, H-13 client spec | H-11 after H-03; H-12 after H-11; H-13 skeleton now, filled as pieces land |
| E. Plain and pickers | H-14 Plain capability fixes, H-15 pick/prompt ladder, H-16 colour roles, H-17 driver and scenario slimming, H-18 hygiene tooling | H-14 after H-04, H-05; H-15 after R-PLAIN-PROMPT and H-03; H-17 after Plain lands and H-05; H-18 after H-01 |
| F. Plain keys | PK-1..PK-6 | R-PLAIN-GATE and H-02 |
| Later | D-1..D-7, R-STRAT-RICH | as stated |

## 4. Phase A: foundations

### H-00: Test baseline (S, P0)

- **Depends on:** none.
- **Context:** `tests/errors.rs::normal_and_piped_apply_failures_keep_original_details` (line \~477) blocked every full-gate claim since baseline `fc47bcf`. It was removed, then re-added in simplified form; that version is unreviewed.
- **Target Files:** `tests/errors.rs`.
- **Constraints:** do not assert Git's exact error wording or version-specific output. Assert structure only.
- **Acceptance:**
  - Review the re-added test. It asserts that for an apply failure the resulting `UserError` has a non-empty `summary`, that `detail` contains the process stderr, and that the original failure details survive both the normal and piped paths. No wording assertions.
  - The IMM-3 mapping table (one test per stderr substring row) still has coverage elsewhere; if not, add the missing rows.
  - `make test` is green on a clean tree, run for real, and the result is recorded in the commit message.
- **Verify:** `make test`.

### H-01: Docs diet and policy (M, P0)

- **Depends on:** none.
- **Target Files:** `AGENTS.md`, `TASKS.md`, `ui-test-plan.md`, `docs/SCREENSHOTS.md`, `docs/RELEASING.md`, `clients/vscode/NOTES.md`, `REVIEW_REPORT.md`, `docs/evidence/*`, `docs/archive/*`.
- **Constraints:** text only; no source changes. Do not delete anything that is the only record of an open issue.
- **Acceptance:**
  1. **Policy text.** Add G1..G9 (or a link to this list's §1) to `AGENTS.md` and the §7 ticket template: text-only evidence, test policy, snapshot rule, generated-artifact rule, delete-don't-deprecate, fake-trust rule. Add a "Phase gate" line to the template (run `make hygiene` at phase end; see H-18).
  2. **Screenshot hygiene.** Move any screenshot capture code to `scripts/readme-shots/` with a human-only `make readme-shots`. Rewrite R-REL-2's "cheap deterministic CI screenshot capture" as a manual human task. Keep `docs/SCREENSHOTS.md` marked human-only. Mark every PNG-citing evidence passage in TASKS.md and evidence docs as historical (a one-line banner is enough); do not rewrite their contents. Strip PNG mentions from `docs/evidence/real-file-client-spike.md` (keep the file; it is the evidence for host-behaviour assumptions).
  3. **Cap completed tickets** to the G6 format. Keep §2 correctness contracts (IMM-\*) as short behavioural rules, each pointing at its enforcing test by name; where no test is named, record that as a gap in one line.
  4. **Strip "both clients native"** from R-SELECT-OPS, R-4.1a, R-7.3a, R-OP-STATE and R-TODO. Tick R-4.1-head-edit once H-00 is green and its verification re-run is recorded.
  5. **Extract before deleting `clients/vscode/NOTES.md`.** Move the host behaviours it alone records (F5 interception, find-widget and QuickPick focus handling, fold chords, mirrored-shortcut results) into `docs/evidence/vscode-host-behaviours.md` as short text facts, each with the Codium version it was observed on. Carry the note that the F5 interception cause is "unproven" into a one-line ticket. Then delete NOTES.md.
  6. **Review report.** Audit `REVIEW_REPORT.md`: for every finding (A01..A34) record its status and where TASKS.md tracks it; grep for `remaining`, `deferred`, `follow-up`, `unresolved`, unchecked boxes; copy any open item into TASKS.md as one line (known candidates: A11 vs R-EDIT-REAL, BUG-READ-1). Then empty the file to a header plus an empty "Open findings" template for future reviews. List the findings audited in the commit message.
  7. Add the "separate commit for goldens" rule to AGENTS.md.
- **Verify:** `git diff --check`; grep confirms no active (non-historical) ticket or doc instructs an agent to capture, open or inspect an image; grep confirms no ticket says "both clients" native.

### H-02: Server-side wire recorder (S)

- **Depends on:** H-01 (policy only).
- **Target Files:** `src/server.rs` (or a new `src/wirelog.rs`) (confirm), `tests/wirelog.rs` (new), `tests/fixtures/lsp_traces/`.
- **Why:** no bundled-client `initialize` or `didChange` capture exists, and VSCode's `trace.server` setting is not contributed. One recorder on the server side captures every client identically, including library-generated traffic.
- **Constraints:** off by default; enabled by an env var (for example `MAGIT_LSP_WIRELOG=<path>`). JSONL, one record per message with direction, timestamp, and the raw JSON. Redact home directories and usernames. No secrets: prompt answers with `secret:true` and askpass values are never logged. Must not change timing-sensitive behaviour or write to stdout.
- **Acceptance:**
  - Records both directions for requests, responses and notifications, in wire order.
  - A test replays a recorded session and checks it round-trips through the JSON-RPC framing.
  - Real traces captured and committed as fixtures: `initialize` and one `didChange` for bundled Neovim, bundled VSCode, plain Neovim (`vim.lsp.start`, no plugin), and Helix (re-record only if the existing JSONL is lost). Mark any reconstructed fixture "reconstructed"; reconstructed traces never count toward conformance.
  - Also capture, for PK-1, Helix and Neovim `didChange` shapes for: insert-mode single character, selection replace, paste of one and of several characters, rapid typing, undo right after a server repair edit. Missing shapes are listed as not captured.
  - Convert the captured initializes into preset fixtures: `initialize-helix-25.07.1.json`, `initialize-neovim-0.12.6.json`, `initialize-vscode-<ver>.json`.
- **Verify:** `cargo test --test wirelog`; fixtures parse as JSON; a manual capture run per client recorded as text.

## 5. Phase B: capability model

### H-03: Typed declaration and dead-code purge (M)

- **Depends on:** H-00.
- **Target Files:** `src/presentation/mod.rs`, `src/protocol.rs`, `src/dispatch.rs`, `src/ui.rs`, `src/presentation/custom.rs`, `lua/magit-lsp/config.lua`, `lua/magit-lsp/init.lua`, `clients/vscode/lib/session.js`, `tests/plain_core.rs`.
- **Constraints:** compatibility is not a goal. Apply rule G5: delete only what has no real consumer. Confirm each deletion with a grep of both clients and the tests before removing.
- **Acceptance:**
  1. Add a typed `MagitCapabilities` struct (serde, `deny_unknown_fields`) replacing the hand-parsed `negotiate()` over `Value`. Both clients' blocks are validated against it in tests. Unsupported variants with no consumer (`keys: forwardAll`, `render: full`, `folds: native`) are deleted; a field left with one possible value is removed from the block.
  2. Delete from `lua/magit-lsp/config.lua` the legacy `capabilities.prompts`, `placements`, `editorBridge`.
  3. Delete verified-unused code: `PromptUi::multi_choice` (nothing emits it; re-add when needed), `process_log_id` if truly unused, `Effect::OpenView` if only `PushView` is produced, the legacy `KeymapDelivery::parse` and `KeyModifiers::parse` paths superseded by the typed block, the duplicate test `RecordingUi` (keep one shared implementation).
  4. `ActionTarget` serialises in camelCase (`nodeId`, `mutationHash`) like the rest of the protocol; update Plain tests and fixtures.
  5. Replace `RichCapabilities::command_requirement`'s hard-coded command-to-effect match with a table derived from commands' declared requirements (the recording `Ui` later verifies it); until the recording `Ui` exists, add a unit test that every effect-emitting command appears in the table.
  6. Audit VSCode's declared `layout: ['main','side','bottom','float']`: list which placements the adapter actually honours (`views.js`, `effects.js`) and declare only those. A false declaration defeats the server's fallback.
  7. Add a **checked emitter** in the presentation seam: in test builds any effect, prompt kind or placement not declared by the connection hard-fails; in production it degrades to the documented fallback.
- **Verify:** `cargo test` (all targets); `make test-lua`; `node clients/vscode/test/test.js`.

### H-04: Quirks table (M)

- **Depends on:** H-02 (real traces).
- **Target Files:** new `src/quirks.rs` and `quirks.toml` (shipped in the binary) (confirm layout), `src/presentation/mod.rs`, `src/server.rs`, `tests/quirks.rs`.
- **Design:**

  ```toml
  [quirk.assume-document-changes]
  summary    = "Applies WorkspaceEdit.documentChanges without declaring it"
  capability = { path = "workspace.workspaceEdit.documentChanges", set = true }
  evidence   = "initialize-neovim-0.12.6; util.lua apply_workspace_edit"
  
  [quirk.apply-result-unreliable]
  summary    = "applyEdit returns applied:true even when a stale edit was skipped"
  workaround = "reconcile-by-echo-hash"
  model      = "silent-stale-apply"
  
  [quirk.decomposed-changes]
  summary    = "One edit arrives as many ranged changes in one didChange"
  workaround = "sequential-change-apply"
  model      = "decomposed-changes"
  
  [client.neovim] match = { name = "Neovim", version = ">=0.10" }
                  quirks = ["assume-document-changes", "apply-result-unreliable"]
  [client.helix]  match = { name = "helix" }
                  quirks = ["decomposed-changes"]
  ```
- **Constraints:**
  - Each quirk has up to three effects, all keyed by one id: a capability override (add or remove), a named server workaround, and a named fake-editor behaviour profile. Shared quirks apply to several editors by listing the id.
  - Matching: `clientInfo.name` and version range, or a capability predicate (for example "declares `experimental.magit`"). Plain Neovim and bundled Neovim both report `Neovim`; the predicate tells them apart. VSCode always reports `Visual Studio Code` (the extension overrides it), so forks are indistinguishable.
  - Resolution order: declared/derived capabilities, then quirk overrides, then user config. User config can force a quirk on or off for forks. The optional `experimental.magit.effective` echo reports the result.
  - Quirks default to conservative behaviour. Cheap, always-correct workarounds (echo-hash reconciliation) are applied universally and not gated by a quirk.
  - Seed with the two facts in evidence: Neovim honours `documentChanges` undeclared (version check skipped for every `documentChanges` entry after the first; stale edit returns `applied:true`), and Helix decomposes edits into ranged changes and returns `applied:false` on stale versions.
- **Acceptance:** every quirk with a `model` is exercised by a fake-editor test (H-05); every quirk with a `workaround` has a server test; a generated spec appendix "Known client behaviours" lists quirks, evidence and workarounds (consumed by H-13); an unknown client gets no quirks.
- **Verify:** `cargo test --test quirks`.

### H-05: Capability-model fake editor (L)

- **Depends on:** H-02, H-03, H-04.
- **Target Files:** move `ClientPump` out of `tests/plain_harness.rs` into `tests/common/` (confirm), `tests/common/fake_editor.rs`, `tests/plain_harness.rs`, new `tests/common/editor_model/`.
- **Known gaps to close** (verify each against the current files first): `ClientPump` is private to one test file; most server-originated requests in tests are scripted rather than emitted by the real server; the pump only speaks Plain (no rich view model); `FakeEditor` hard-codes request ids 71 and 72. Request/response ordering, decomposition and echo-order tests already exist and must keep passing.
- **Design:**
  - A client model is a **base plus modules**. Bases: `plain-min` (only `applyEdit` and `codeAction`) and `rich-min` (core render/key, `layout:[main]`, no optional effects or prompts). Modules (each: a `declares` fragment of initialize capabilities, a `behaves` part servicing the implied server traffic, and an `// anchor:` comment): `documentChanges`, `showDocument`, `completion`, `workDoneProgress`, position encodings, `foldingRange`, `semanticTokens`, rich `effects`, rich `prompts`, rich `layout`, `keys: dynamic|static`.
  - **Strict mode:** if the server uses anything the model did not declare, the test fails.
  - Behaviour profiles are generated from the quirks table (`model =` entries) plus existing ones (`debounced-didchange`, `reorder-echo`, `crlf-normalise`, `utf16`, `no-showDocument`, `no-documentChanges`). Add `silent-stale-apply` (skips a stale edit but returns `applied:true`) and `neovim-accepts-undeclared-documentChanges`.
  - Rich model: applies `view/render` (full, edits, metadata-only), acks, resyncs, and answers prompts, driven by `tests/data/client-contracts.json` vectors.
  - Request ids are allocated dynamically.
  - Named presets built from recorded traces: Neovim 0.12.6 stock, Helix 25.07.1, bundled Neovim, bundled VSCode. Presets always run.
- **Acceptance:** the existing 12 harness tests pass on the shared pump; a test where the real server emits `workspace/applyEdit` after a Plain `didOpen` and the pump services it; strict-mode failure demonstrated by a test that deliberately uses an undeclared capability; every quirk profile has a self-test; `rich-native`, `rich-doc` (rich client declaring no pick/prompt shapes) and `plain` profiles exist.
- **Verify:** `make test-rust FILTER="--test plain_harness"`.

### H-06: Lattice and combination tests (M)

- **Depends on:** H-05, R-PLAIN-PROMPT (recording `Ui` and its requirements table).
- **Target Files:** `tests/capability_lattice.rs` (new), `tests/common/editor_model/`, `tests/ui/plain-presets/` (new, Neovim presets) (confirm).
- **Acceptance:**
  1. **Lattice property.** For each conformance scenario S with requirement set R (derived from the recording `Ui` table) and any capability set C: if R is satisfied directly or via a fallback rung, the final repo state equals the full-capability reference run; otherwise there is no mutation, the client receives a readable message, and the document stays consistent.
  2. **Combination selection.** Always run the named real-trace presets. Add a pairwise covering set over the remaining axes with explicit constraints (for example document prompts need `applyEdit`). Add a short hand-written list of known interactions: position encoding x edit ranges, `documentChanges` x echo ordering, `showDocument` x commit-template refresh, mixed-rung command chains, progress x cancel. Measure cost; report combination and scenario counts.
  3. **Neovim presets over RPC.** A child Neovim (no tmux, no magit plugin) started with `vim.lsp.start{capabilities = …}` runs a small set of presets: minimal (`applyEdit`+`codeAction`), plus `showDocument`, plus completion, plus progress, full. Remember Neovim applies some features it did not declare; the strict fake editor, not Neovim, is what proves the server never emits an undeclared feature.
  4. The generated "what you get for what you declare" table (consumed by H-13).
- **Verify:** `make test-rust FILTER="--test capability_lattice"`; one scripted Neovim preset run recorded as text.

## 6. Phase C: VSCode

### H-07: Real extension-host spike (S)

- **Depends on:** H-01.
- **Goal:** decide whether a real extension host under a private Xvfb replaces the hand-written `vscode` and `LanguageClient` fakes (`clients/vscode/test/fake/vscode.js`, `languageclient.js`, about 820 lines plus `integration.js`).
- **Measure** one launch running a list of in-host cases with the real `LanguageClient` and the real server: launch time, memory, and flake rate over 10 consecutive runs. Several seconds per launch is acceptable.
- **Answer these questions as recorded text facts** in `docs/evidence/vscode-host-behaviours.md`, each with the Codium/VS Code version observed:
  1. Does CDP `Input.dispatchKeyEvent` with explicit `key`, `code` and `windowsVirtualKeyCode` trigger contributed keybindings (including `when` clauses and a chord such as `ctrl+k ctrl+l`)? If not, does XTEST on a private Xvfb with no window manager work?
  2. Are native Save / Don't Save / Cancel dialogs visible as DOM text via `Runtime.evaluate` and `innerText` when `window.dialogStyle` is `custom`, and can they be clicked by role?
  3. Does the clipboard stay inside the virtual display?
  4. Does closing a tab emit `onDidCloseTextDocument`? (spike evidence says no) Do `tabGroups.all` tab objects keep stable identity across reads? What does `setTextDocumentLanguage` fire? Does `showQuickPick` return `undefined` on dismiss? Does the real language client apply `uriConverters` to every `textDocument` URI?
  5. Do `ctrl+t` and `ctrl+shift+x` collide with workbench shortcuts, and are they shadowed only by the `when` clause?
  6. Does a top-level `"//generated"` key in `package.json` survive `vsce package` and extension loading?
  7. What `clientInfo` does the language client send? (the extension overrides it to `Visual Studio Code`)
- **Deliverable:** a one-page decision: delete the fakes (preferred) or keep a minimal stub plus a host-contract suite. If a minimal stub survives, it is built as a minimal host plus per-capability modules (tabGroups, file-system provider, semantic tokens, clipboard), each module validated by an assertion that runs on the real host (rule G7).
- **Verify:** the evidence file exists and every answer is text; no image artifacts.

### H-08: E2E runner redo (M)

- **Depends on:** H-07.
- **Target Files:** `clients/vscode/test/e2e/run.mjs`, `real-file-lifecycle.mjs`, `real-file-spike.mjs` (confirm), new `scripts/xvfb-run-native` or equivalent.
- **Context:** the current runner passes through `DISPLAY`, `DBUS_SESSION_BUS_ADDRESS` and `XDG_RUNTIME_DIR`, sets `keyboard.dispatch: keyCode` to compensate for a Colemak desktop, sends some keys through XTEST/xdotool, and a spike calls `bspc` against the real window manager. CI is qwerty; the desktop is not.
- **Acceptance:**
  1. **Private Xvfb every run**, started by the runner on a free display, with `setxkbmap -display :N us` executed explicitly every time (no meta-test needed). Remove the `keyboard.dispatch: keyCode` compensation.
  2. **Hermetic environment.** Force `DISPLAY` to the private display. Drop `DBUS_SESSION_BUS_ADDRESS`, `WAYLAND_DISPLAY` and the real `XDG_RUNTIME_DIR`. Pass `--ozone-platform=x11`. Remove every `bspc` call. Use `--user-data-dir` and `--extensions-dir` under `/tmp` (tmpfs where available) and delete them after each run. No Docker.
  3. **Keys through CDP** (`Input.dispatchKeyEvent`) wherever H-07 proves it works; XTEST only as a fallback on the private display. Never `xdotool` against the real display. A `press(binding)` helper parses a VSCode key string (`shift+/`, `ctrl+enter`) into `key`/`code`/`windowsVirtualKeyCode` from a US-layout table of about 40 entries.
  4. **Resource limits.** Run under `systemd-run --user --scope` with low `CPUWeight`/`CPUQuota` so one cgroup can be killed if the supervisor dies; keep the existing PID/start-time census and verify zero owned processes at cleanup.
  5. Dialog handling by DOM text dump and click-by-role, not coordinates.
  6. Native tier is opt-in locally (`MAGIT_NATIVE=1`), always on in CI.
  7. Document the product limitation that generated bindings such as `shift+/` for `?` assume a US-like layout (one README line).
- **Verify:** `MAGIT_NATIVE=1 npm --prefix clients/vscode run test:e2e` on a machine whose desktop layout is not US, with a recorded text log showing `setxkbmap -query` of the private display; zero leftover processes.

### H-09: Host cases, seams and fake deletion (L)

- **Depends on:** H-07, H-08.
- **Target Files:** `clients/vscode/lib/hooks.js` (new), `clients/vscode/lib/debug.js` (new), `clients/vscode/test/e2e/cases/*.js` (new), `clients/vscode/test/e2e/lib/*.js` (new), `clients/vscode/test/e2e/suite.js`, `clients/vscode/test/integration.js`, `clients/vscode/test/fake/*`.
- **Acceptance:**
  1. **Test seams.** Mirror `lua/magit-lsp/hooks.lua`/`debug.lua`: `lib/hooks.js` holds no-op hook functions called from product code; `lib/debug.js` fills them when the test environment variable is set. Provide `snapshot()` and `idle()` with a state schema shared with Neovim where meaningful: active view, view stack, render version and acked version, keymap scope/capture/pending, drafts (run id, uri, version, saved version, dirty), recent messages, and tab/group dump (uri, dirty, group). `idle()` = last render acked and no request in flight; no sleeps.
  2. **Split `suite.js`** (about 1,800 lines under an env-flag ladder A06/A29/A33/A34/EDIT\_ONLY/HEAD\_ONLY). Step 1, mechanical: move helpers (`settle`, `waitFor`, `workspaceFiles`, CDP and evidence writers) to `e2e/lib/`; move each section to `e2e/cases/<name>.js` using the `run({extension, evidence, repo, cdp, waitFor, checkpoint, …})` signature already used by `head-operations.js` and `real-file-native.js`; reduce `suite.js` to about 100 lines selecting cases through one `MAGIT_E2E_CASES=a,b,c` list; replace `run.mjs`'s evidence-file ladder with a per-case manifest. Compare evidence JSON before and after; behaviour must not change. Step 2: decide membership.
  3. **One-launch smoke set** (fast, every CI run): status render and fold through a real keybinding including a chord; cursor and selection stage/unstage including a selection that ends at column 0 of the next line (assert only the intended line is staged); log push/pop with cursor restored; commit menu `-a`, commit preflight confirm, editor open and explicit confirm; last-owned-tab close confirms saved text, and explicit abort; optionally text-mode toggle.
  4. **On-demand adapter-risk set** (run when `drafts.js`, `edit.js` or `session.js` change, on a Codium version bump, and in the CI full run): native dirty-close Save/Don't Save/Cancel, multi-group tab behaviour, lifecycle (reload, close-window, folder change).
  5. **Move feature acceptance out of native.** `head-operations` and most of the A34 selection variants (the 240 real-Git cases already run in Node) move to Rust or Node tests; delete the theme case.
  6. **Port, then delete.** Port everything `integration.js` and the fakes still cover into in-host cases (render apply, prompts for each declared kind, effects for each declared effect, text mode, queued refresh, quit scoping, disconnect cleanup), then delete `fake/vscode.js`, `fake/languageclient.js` and `integration.js`. Keep the pure no-fake unit tests (`render`, `menu`, `prompt`, `context`, `foldkeys`). If H-07 decided to keep a minimal stub, apply H-07's modular rule instead.
  7. **Declaration honesty.** One in-host loop over every declared effect, prompt kind and layout asserts the adapter handles it; a server test asserts it never emits an undeclared one (H-03's checked emitter).
  8. **Binding coverage loop.** For every contributed binding, press it with `press(binding)` in its scope and assert the expected `view/invoke` or `view/key`. Start with the `status` and `common` scopes.
  9. **Wire goldens.** Use the server-side recorder (H-02) output for client-originated traffic of the smoke set: normalized, small, paired with semantic assertions (a render is acked after apply; response before follow-up notifications; progress create, begin, end ordering). Library traffic (`didOpen`, `didChange`, `initialize`) comes from the same recorder.
  10. Evidence is text only (G1): document text, `snapshot()` JSON, tab dump, recorder JSONL, repo state.
- **Verify:** `MAGIT_NATIVE=1 npm --prefix clients/vscode run test:e2e`; `make vscode-test`; evidence files listed as text paths.

### H-10: Keybinding generation and compaction (M)

- **Depends on:** none (independent generator work); the denylist test needs only the generated file.
- **Target Files:** `clients/vscode/scripts/gen-keybindings.mjs`, `src/` keymap manifest generator (`magit-lsp keymap --format vscode`) (confirm), `clients/vscode/package.json`, `.gitattributes`, `Makefile`.
- **Findings to act on** (VERIFY each before deleting): about 270 contributed keybindings in about 1,650 lines; about 22 common commands are repeated across 8 scopes because inheritance is flattened per scope; the first 28 entries are guarded by `magit.scope == 'common'` but `deriveContext` never produces scope `common` (it yields `status`, `menu`, `log`, `log.select`, `select`, …), so they can never fire; the `commit-message` scope block (about 25 entries) and the `resourceScheme == magit-commit` commands, menus and `magit.confirmEdit/cancelEdit/closeEdit` look dead because commit editing is now a real file and the tests assert `magit.active` is false for edit contexts; paired `ctrl+t` entries differ only in the `magit.textMode` term.
- **Acceptance:**
  1. Emit `common` bindings once, guarded only by `magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible`. After it emit each scope's *differences* only (VSCode lets later matching rules win). Add an explicit negation entry only where a scope removes a common binding. Merge the paired `ctrl+t` entries into one.
  2. Delete the verified-dead `common`-scope block, the `commit-message` scope block, and the `magit-commit` commands/menus (and `lib/edit.js` references) if confirmed unused.
  3. The generated block in `package.json` is marked per G4 (`"//generated"`, `linguist-generated`), still committed because the extension ships it, with `gen-keybindings --check` as the drift check in `make vscode-test`.
  4. **Workbench denylist test** over the generated bindings: `ctrl+p`, `ctrl+shift+p`, `ctrl+w`, `ctrl+tab`, `f1`, `alt+tab` and similar are never bound; any exception (for example `ctrl+t`, `ctrl+shift+x`) is listed with a reason and the `when` clause that scopes it. Mainly a development guard when adding profiles.
  5. If the Rust generator already emits Neovim bindings, check whether Lua's chord translation (`lua/magit-lsp/keys.lua`, 320 lines) can be reduced to a table lookup; otherwise leave it.
- **Verify:** `make vscode-test` (includes the drift check); `node clients/vscode/test/test.js`; generated entry count recorded before and after.

## 7. Phase D: schema, examples and the client spec

### H-11: Schema generation (M)

- **Depends on:** H-03.
- **Target Files:** `src/protocol.rs`, `src/menu.rs`, `src/view.rs`, `src/dispatch.rs`, `src/editor.rs`, `Cargo.toml`, a generator binary or xtask, `docs/schema/` (generated), `clients/vscode/types/*.d.ts` (generated), `lua/magit-lsp/types.lua` (generated LuaLS annotations).
- **Hazards from the source audit and the intended handling:**

  | Hazard | Handling |
  | --- | --- |
  | `ViewKind` has a custom `Serialize` (wire is a string like `menu.commit`) | `#[schemars(with = "String")]` or a hand-written schema listing kinds |
  | `menu.rs` uses `serialize_with` and `flatten` | `#[schemars(with = …)]` on the item list; check flatten output |
  | `untagged` enums: `PromptValue`, `PromptChoiceAnswer`, `PlainActionTarget`, `editor.rs:56` | `PlainActionTarget` becomes an internally tagged enum (compatibility is not a goal); the prompt value types are shape-based by nature and stay `anyOf` |
  | Serialize-only `EditConfirmation`, deserialize-only `EditConfirmParams` | generate each in the right mode per direction; verify `rename_all_fields` support in the pinned schemars version |
  | `Option<Value>` fields (`args`, prompt `default`) | accept as `any`, or specialise prompt `default` into a union |
  | hand-parsed `experimental.magit` block | typed in H-03 |
  | methods are trait impls and consts, not data | add a small hand-written method table: method, direction, kind (request/notification), params type, result type |
- **Acceptance:**
  1. JSON Schema generated from the Rust types (authoritative) plus the method table, committed under `docs/schema/` with a CI up-to-date check.
  2. Generate a `.d.ts` for the VSCode extension and run `tsc --checkJs` over the existing JS in `make vscode-test` (TypeScript is already a devDependency). A renamed or missing field breaks the build.
  3. Generate LuaLS `---@class` annotations for the Neovim client (editor diagnostics only; no runtime validation in Lua).
  4. Generate the payload tables for the client spec (H-13) from the same schema.
  5. A test validates every message in the recorded fixtures and example files (H-12) against the schema.
  6. Runtime data tables duplicated across clients: only the effect list and the prompt list are byte-identical today. Generate those from the typed block definition. The key-notation translators, token-to-highlight maps and prompt widget code are structurally different and stay separate; do not force them into shared JSON without a measured reason.
- **Verify:** schema drift check; `make vscode-test`; Rust schema-validation test.

### H-12: Example files (S)

- **Depends on:** H-11.
- **Target Files:** `docs/examples/<level>-<name>.jsonl` (new), `tests/examples.rs` (new).
- **Design:** each line is `{dir, message, note?}` where `dir` is `c2s` or `s2c`. These files are the single source for every JSON example in the client spec.
- **Acceptance:**
  - Every message validates against the schema.
  - A harness test replays the client-to-server lines against the real binary and diffs normalized server lines; examples therefore double as goldens (subject to G3).
  - Initial set mirrors the spec levels: minimal initialize and Plain open (Level 1), a code-action stage (Level 2), a document prompt (Level 3), the rich open/render/ack/key loop with progress ordering (Level 4), a commit edit confirm and abort.
  - Examples captured from real traces are marked `source: recorded`; others `source: written`.
  - The spec generator inlines them; hand-written JSON in the spec prose is not allowed.
- **Verify:** `cargo test --test examples`.

### H-13: Client specification (L, start the skeleton now)

- **Depends on:** H-11, H-12 (skeleton earlier); fills in as H-04..H-06, H-14, H-15 land. Replaces and supersedes `docs/CLIENT_SPEC.md`.
- **Goal:** a human can read it and know exactly how to implement a client, from basic LSP setup to a rich client.
- **Structure (leveled; each level is a complete working client):** 0. Connect: initialize, capabilities, document sync, position encodings.
  1. Plain read-only: the status document, `applyEdit`, echo handling.
  2. Plain actions: code actions, `executeCommand`, selections.
  3. Plain extras: document prompts, completion, `showDocument`, commit editing, progress.
  4. Rich: render/ack/resync, key delivery, effects, native prompts and picks, menus, text mode, real-file drafts.
- **Each level contains:** what you send and receive; the capabilities to declare and what degrades if you don't; one worked exchange (from `docs/examples`); and a conformance checklist whose items are conformance scenario ids.
- **Also:** a capability-to-rung decision table generated from the lattice tests (H-06); an appendix "Known client behaviours" generated from the quirks table (H-04); a capability-classification table per G8; per-feature status (implemented, target, planned) with a pointer to the test that proves it, never mixing design intent into current behaviour; small reference clients (the 30-line `tests/ui/plain_init.lua`, a Helix TOML) as Level 0-2 examples; the Neovim `applyEdit`/`documentChanges` facts (G9) and the one-document-per-`applyEdit` rule.
- **Constraints:** payload tables and JSON examples are generated, never hand-written. Narrative is hand-written. Every claim cross-checked against `src/protocol.rs` and the tests.
- **Verify:** link and example validation (H-12); generated sections up to date; documentation review.

## 8. Phase E: Plain fixes, pickers, colour, drivers, hygiene

### H-14: Plain capability fixes (M)

- **Depends on:** H-04, H-05.
- **Target Files:** `src/presentation/mod.rs` (`PlainCapabilities::from_lsp`, `content_action`), `src/presentation/plain.rs`, `src/server.rs`, `tests/ui/plain_init.lua`, `tests/test_magit_lsp.lua`.
- **Evidence behind this ticket:** stock Neovim 0.12.6 does not declare `workspace.executeCommand` nor `workspaceEdit.documentChanges`, yet applies `documentChanges` (the version check is skipped for every entry after the first), and returns `applied:true` for a skipped stale edit (source-derived by a research agent from `util.lua` and the `workspace/applyEdit` handler, not executed here; the mini.test below confirms it). Helix 25.07.1 declares `documentChanges` but not `window.showDocument`, `semanticTokens` or `foldingRange`, returns `applied:false` on stale versions, and sends decomposed ranged changes.
- **Acceptance:**
  1. Gate Plain actions on `textDocument.codeAction` only; remove the `executeCommand` requirement from `from_lsp` and `content_action`. The `executeCommand` workaround in `plain_init.lua` may then be removed.
  2. Treat Neovim as `documentChanges`-capable through the H-04 quirk. Versioned edits are preferred wherever a client applies them; an unversioned `changes` edit applied to changed text would corrupt it.
  3. Classify every capability read in `from_lsp` per G8 ("declaration is the feature" versus "declaration only describes sub-features") in a code comment and in the spec table.
  4. **Echo watchdog.** When `applyEdit` returns `applied:true` and no echo `didChange` arrives within a bounded time, the document must not stay in `Applying` forever (reconcile against the next observed snapshot or re-render). Test with the `silent-stale-apply` profile.
  5. **In-process mini.test** exercising the server-to-client `workspace/applyEdit` path (not `util.apply_workspace_edit` directly): with a Lua in-process server (`vim.lsp.start{cmd = function(dispatchers) … end}`), cases for advertises `documentChanges` yes/no x edit version matching/stale: matching edits change the buffer; stale edits do not; `applied` is `true` in all four; plus a case with two `documentChanges` entries where the second is stale (check the index-based exemption). Use the real `changedtick` after attach, keep stale versions positive.
  6. **Helix commit rung decision**, documented and implemented: Helix does not declare `showDocument`. Options to evaluate and choose among, recording the outcome in the Helix smoke notes: (a) message the path and accept any `didOpen` of the announced `COMMIT_EDITMSG` URI as the editor attaching; (b) mark commit commands `Unavailable` when `showDocument` is absent (current table behaviour); (c) a manual-open path with explicit Confirm and Abort code actions. Also evaluate a navigation fallback: a row such as "Commit message ready: go to definition here" whose `textDocument/definition` returns the `COMMIT_EDITMSG` location (navigation is pull-based and client-initiated; the server cannot push it). VERIFY in Helix whether this and native goto-file work. Make the fallback a rung after `showDocument`.
  7. Keep: one document per `applyEdit` (G9).
- **Verify:** `make test-rust FILTER="--test plain_core --test plain_harness --test plain_render"`; `make test-lua`.

### H-15: Prompt presentation and pick ladder (M)

- **Depends on:** R-PLAIN-PROMPT (document prompt renderer), H-03.
- **Target Files:** `src/protocol.rs`, `src/ui.rs`, `src/commands.rs`, `src/presentation/*`, `lua/magit-lsp/prompt.lua`, `clients/vscode/lib/prompt.js`, `clients/vscode/lib/requests.js`, TASKS.md §3.6/§3.7.
- **Acceptance:**
  1. Add a "Prompt presentation" principle to §3.6: the prompt is semantic, presentation is per client, undeclared kinds fall back to the document renderer, and the document renderer works over both transports (`view/render` and `applyEdit`). Remove the blanket "no client picker" wording; the rule becomes "clients never filter, parse or resolve Git state".
  2. Introduce one abstract `pick` interaction (`choice` and `multiChoice` are special cases) declared in the capability block: `purpose`, `multi`, `allowFreeText`, preselection, items `{id, label, description?, detail?, group?}`, optional `fallbackView`. Static delivery only. One filter owner per pick (the client filters static lists; the server never also filters).
  3. **Ladder:** native pick (declared), server-rendered view (log select or rows), code actions over a document. The server never emits an undeclared shape; it picks the next rung. Replace the current behaviour for an undeclared prompt (a warning message and a dead command) with the fallback.
  4. **Selection rule:** a target reachable from HEAD uses log select (fixup, squash, reword, interactive rebase with nothing at point); everything else uses a text pick that accepts free text (validated server-side with `git rev-parse --verify`) with the at-point value preselected. The picker always appears; at-point is a preselection, never a skip. In a region or multi-selection, preselect several. An item-level "open in log" escape is allowed. IMM-6 smallest-node-under-cursor rule is unchanged.
  5. Rich presentation of document-backed prompts: field text is the store's current value, so a diff never touches it; refreshes and repairs for the underlying view queue while a prompt view is active (as text mode already does).
  6. **Overrides:** user preference inside the declared ceiling wins (for example `ui.picker.history = "log" | "native"`); exceeding the ceiling needs an explicit per-client config key keyed by `clientInfo.name`; otherwise fall back with a message.
  7. Picks are re-resolved and validated at execution (names can move), as `ActionTarget` does.
  8. **Fix two Lua prompt bugs:** a custom `choice` entry reuses `ask_text`, which prefills with `params.default`, but for a choice that default is an item id; and an empty `inputsecret` result cancels in Lua while VSCode returns an empty string as a valid answer. Decide one behaviour and test both clients against the shared vectors.
  9. Neovim declares static picks only (`vim.ui.select`); lists over a cap are truncated with a marker or pushed to the document rung. Add the secret/askpass line to the support table: rich-only.
  10. Update `tests/data/client-contracts.json` vectors and add the mixed-rung chain tests from H-06.
- **Verify:** `cargo test`; `make test-lua`; in-host VSCode cases (H-09); mixed-rung chain test with cancel at each step asserts no mutation, caller restored, view-stack depth unchanged.

### H-16: Colour roles and token legend (S)

- **Depends on:** H-01.
- **Target Files:** `src/semantic_tokens.rs`, `lua/magit-lsp/highlights.lua`, `clients/vscode/package.json` (`semanticTokenTypes/Scopes`), `clients/vscode/lib/semantic.js`, `scripts/color_spike.lua`, `Makefile`, `ui-test-plan.md` §8.4.
- **Principle:** semantic tokens carry no colours; clients map names to their own styling. Theme behaviour is the editor's concern. Test semantic correctness only.
- **Acceptance:**
  1. Delete `scripts/color_spike.lua`, `make color-spike`, and every HTML output (the spike fails at setup, and `tohtml` cannot see LSP tokens). Drop contrast, multi-scheme and "colour changes with colourscheme" assertions from §8.4.
  2. Collapse the 28 token types to about 10 roles (heading, muted, added, removed, changed, path, hash/identifier, ref, key/accent, warning/error) with modifiers that only add attributes (bold, dim, selected, staged). Every emitted type declares its role; a test fails on a missing role. Prefer standard LSP token types where one fits (`comment` for counts, `number` for hashes) so generic editors colour them without configuration; keep distinct custom types only where users plausibly restyle them separately. VERIFY in Helix that unknown custom types render uncoloured.
  3. VSCode currently contributes 5 of 28 types and ignores `tokenTypeGroups`; derive its scopes from the small role table. `semantic.js` maps dots to hyphens; keep consistent.
  4. **Tests:** (a) Rust: the semantic-token response for a fixture view has the expected type and modifiers at known ranges; (b) per-client legend coverage as a data test: every legend type has a Neovim link target or VSCode scope in the allowed list (`nvim_get_hl` with `link=true`, package contributions); (c) one ANSI smoke per client under one built-in scheme: added and removed cells have different non-default foregrounds, using the existing `decoder.py` and `expect style` verb. Truecolour must be on; a prior experiment shows tmux `capture-pane -e` returns `38;2;r;g;b` for a known colour.
- **Verify:** `cargo test --test semantic_tokens` (or the existing token tests); `make test-lua`; `scripts/ui run tests/ui/scenarios/<colour smoke>.scn`.

### H-17: Drivers and scenario slimming (M)

- **Depends on:** H-05; Plain landed (R-PLAIN-E2E).
- **Target Files:** `scripts/ui_lib/*`, `scripts/test_plain_neovim.py`, `tests/ui/plain_init.lua`, `tests/ui/scenarios/*.scn`, `ui-test-plan.md`, new `tests/conformance/` (confirm).
- **Findings:** `scripts/ui_lib` already drives Neovim over tmux plus RPC (`document`, `goto_text`, `snapshot`, `idle` over RPC; only `send_keys` and `capture` use tmux). `scripts/test_plain_neovim.py` is a second, standalone runner with its own subprocess/tmux calls. R-PLAIN-E2E called for one harness (`scripts/ui` client option plus `plain-*.scn`), so this is an omission to repair. Read `test_plain_neovim.py` before changing anything; this report has not seen its contents. `profile:` in `.scn` already means keymap profile, so the presentation axis is a new header `client: rich | rich-doc | plain`.
- **Acceptance:**
  1. Fold `test_plain_neovim.py` into `ui_lib` as `client: plain` (init file plus env); a Plain and a rich scenario are two `.scn` files in one harness.
  2. **Neovim backends:** a child Neovim over RPC (no tmux) for semantic scenarios (`documentText`, `moveToText` via cursor set, `vim.lsp.buf.code_action` filtered by command id for Plain `invoke`); tmux L3 only for what needs a real terminal (key timing, chord timeouts, key encoding, the command line, ANSI colour smoke).
  3. **Matrix slimming:** single combination (80x24, one keymap profile) is the default for new scenarios; `matrix: true` opts in to sizes x profiles. Write `.screen`/`.ansi` only on failure or on explicit request. Existing scenarios keep working.
  4. **Conformance set:** 6 to 8 intent-level scenarios as data (`tests/conformance/*.json` or similar): open status, stage a file, stage a hunk, stage one visual-selected line then unstage non-visually, fold, a menu with an argument, a commit with confirm and with abort (HEAD message asserted), log select confirm and cancel. Steps use verbs `open`, `moveToText`, `select`, `invoke`, `respondPrompt`, `saveFile`, `closeDocument`, `confirm`, `abort`, `expectDocument`, `expectRepo` (shell snippets over repo state). Targets are text locators, never line numbers or cursors; cursor-dependent behaviour is tagged `requires: cursorControl`. Each driver has a thin interpreter in its own language (Rust fake editor, Python `ui_lib`, Node in-host); no cross-language runner.
  5. **`invoke(commandId)` is derived, not a primitive:** rich drivers look up the binding in the generated manifest and `press` it (unbound command fails loudly, enforcing R5); the Plain driver finds the code action by command id (never by title; titles are configurable) and runs it.
  6. **Driver-only properties:** `closeSignal` (`didClose`, tab-closed event, or none), `cursorControl`, `nativeDialogs`. `requires:` is derived from the recording `Ui` table (hand-annotate until it exists, and expect drift).
  7. **Differential check:** run the same conformance scenario on the fake editor and on real plain Neovim; a divergence is a fake bug. The same scenario run rich and Plain must produce the same Git state.
  8. Report a scenario x driver matrix with explicit SKIP(reason); assert every feature has at least one non-skipped driver.
  9. Driver verbs: `start`, `stop`, `press(canonicalKey)`, `documentText`, `moveToText`, `select`, `respondPrompt`, `waitIdle` (never sleep), `snapshot` (text only).
- **Verify:** `make ui-unit`; `scripts/ui run` on the conformance scenarios under `client: rich` and `client: plain`; report attached as text.

### H-18: Hygiene tooling (S)

- **Depends on:** H-01.
- **Goal:** find dead code and size creep at phase ends and review time, never per ticket.
- **Acceptance:**
  1. Add to the dev shell and wire into `make hygiene` (report-only first): `cargo machete` (list false positives in `[package.metadata.cargo-machete]`); `knip` for `clients/vscode` (entry points: `extension.js`, tests, scripts); `luacheck` with a `.luacheckrc` declaring `vim` globals; a size report of the 20 largest non-generated files with a soft cap; greps for `allow(dead_code)` and `keep-for:` annotations (the current way dead code is retained).
  2. Baseline existing findings into allowlist files, each entry with a reason, so the first run is not noise. Later tighten to fail on new findings.
  3. Workflow docs: AGENTS.md "Phase-end and review" section naming `make hygiene`; the §7 template's phase-gate line (done in H-01); a hygiene-run entry in R-REL-4's checklist; review procedure: every finding is deleted, allowlisted with a reason, or ticketed.
  4. Optional weekly CI schedule, report-only.
- **Verify:** `make hygiene` runs and exits 0 with the baseline; a seeded unused dependency or dead function is reported (then removed).

## 9. Phase F: Plain keys

*Optional input path for editors that cannot bind code actions to single keys: the user types one character, the server interprets it as the matching Magit key, re-renders, and the render removes the character. Never destructive; code actions remain the only path for destructive commands. Helix is the motivating case. Starts after R-PLAIN-GATE; supersedes `magit-lsp-plain-keys-tasks.md` structure (nine tickets to six).*

### PK-1: Record edit shapes (S, gate)

- **Depends on:** H-02.
- **Acceptance:** raw `didChange` traces with version numbers from plain Neovim (no plugin) and Helix 25.07.1 for: insert-mode single character, charwise and linewise selection replace, paste of one and of several characters, rapid typing (debounce batching), and undo/redo right after a server repair edit. Only one Helix `didChange` (a single insert at 0:0) is in the current evidence; the delete-then-insert pair the design depends on must be captured or marked reconstructed. Add fake-editor profiles `selection-replace-pair`, `selection-replace-single`, `rangeless-full-text`, `batched-changes`, `paste-multichar`, `undo-after-repair`, each from a recorded trace. Evidence file says which editors produce which shapes and flags anything outside the three accepted patterns (single insertion; one ranged replace by one character; delete then single-character insert at the deletion start in the next notification).
- **Verify:** `make test-rust FILTER="--test plain_harness"`; evidence file reviewed. Do not start PK-2..PK-5 until reviewed.

### PK-2: Net-edit extraction and classifier (M)

- **Depends on:** PK-1.
- **Target Files:** `src/docstore.rs`, `src/presentation/keys.rs` (new), `tests/plain_keys_input.rs` (new).
- **Acceptance:** pure functions, no Git, no I/O. `NetEdit{pre_hash, version_before, version_after, pre_range, inserted}` per `didChange` notification (several ranged changes composed sequentially, rangeless changes diffed by common prefix/suffix, otherwise `Complex`). Classifier states Ready, PendingDeletion, Executing; outputs `Accept{key, position_or_selection, snapshot}`, `Wait`, `Reject(repair)`. Accepts only the three patterns; resolves against the snapshot whose text hash equals the pre-edit text; recognises echoes of server edits by resulting text hash, never by a flag; a lone deletion waits and expires to `Reject(repair)` after `plain.keys.pair_timeout_ms` (default 250) and never pairs with a later unrelated insert; newline, tab, multi-character, paste, empty-range and header edits are `Reject`. Space maps to `Space`, `+` to `Plus`; other printables map to themselves (never `shift+`). Selection conversion: half-open LSP range to inclusive lines, end at column 0 means previous line; encoding units documented in a test. Tests use every PK-1 fixture plus multibyte, CRLF, newline-only ranges, deletions ending at column 0.
- **Verify:** `make test-rust FILTER="--test plain_keys_input"`.

### PK-3: Policy, arming and settings (M)

- **Depends on:** PK-1, H-04, R-PLAIN-PROMPT.
- **Acceptance:** one `destructive: bool` attribute per command (default false), set by an audit of commands that discard, hard-reset, delete or force-push; a CI test runs every command against the recording `Ui` and fails if a command emits a destructive confirm without the flag. Eligibility for character input = `Implemented`, not `destructive`, bound in the active profile. Per-document keys state is off by default; a "Keys: on/off" code action toggles it; a server-rendered header row shows the effective state and its source and any edit to it is repaired; state survives view-stack changes and resets on close; prompt views and the commit file never arm. Settings layering: built-in off, user global `plain.keys.default`, per-client section keyed by `clientInfo.name`, per-document toggle; the quirks-table ceiling (editor edit shapes outside the accepted patterns) overrides all and the header says so. No per-repository setting.
- **Verify:** `make test-rust FILTER="--test plain_keys_policy --test plain_keys_arm --test manifest --test parity"`.

### PK-4: Execution (M)

- **Depends on:** PK-2, PK-3, R-PLAIN-CORE, R-PLAIN-TARGET.
- **Acceptance:** accepted keys resolve through the existing key handler and profile keymap (no second keymap) against the matched snapshot's tree, then dispatch exactly as `view/key` would. While Executing or Applying, further edits are dropped and repaired after the render (no queueing). After a command a repair render restores authoritative text (removing the typed character), diffed against the document store's current text. Chord and menu steps work one character at a time using the view's `pending` state, shown in the document. Ineligible, destructive, `Planned` or unbound keys give a one-line message and a repair (destructive keys point to the code action). Stale targets give the Plain "changed since displayed; refreshed" message and no mutation. No keymap pushes to Plain clients. Scenarios: stage file, stage hunk, stage a visual selection, unstage, `n`/`p`, `c` opens the commit menu then `-a` toggles; each asserts exact Git state.
- **Verify:** `make test-rust FILTER="--test plain_keys_exec"`.

### PK-5: Safety and end-to-end (M)

- **Depends on:** PK-4.
- **Acceptance:** a property test generating random edit sequences (insertions, deletions, pastes, undo-like reinsertions, mixed with renders and echoes) against a real disposable repository; invariant: Git state changes only for an armed, eligible, correctly shaped candidate against a matching snapshot, and any divergence ends with the document equal to authoritative text. Targeted cases: rapid repeat key; server echo never executes; paste and multi-character rejected; edits in another URI ignored; prompt-field edits are field input; commit-file edits never interpreted; keys-off ignores everything; stale pending deletion never attaches to a later insert; undo after repair is repaired and the accepted reinsertion hazard is documented. Plain-Neovim `.scn` scenarios (insert-mode single-key stage, visual-selection replace stage, keys-off typing repaired, destructive key refused) with text evidence. A Helix manual checklist (selection-replace pairing, timeout restore after a lone deletion, keys on/off) recorded as text.
- **Verify:** `make test-rust FILTER="--test plain_keys_safety --test plain_keys_e2e"`; `scripts/ui run tests/ui/scenarios/plain-keys-*.scn`.

### PK-6: Documentation (S)

- **Acceptance:** the client spec (H-13) gains the Plain-keys capability section: what a client must send, accepted edit patterns, key set and exclusions, arming, timeout, undo hazard, never active in prompts or the commit file, settings layering. README mentions it as optional with limits.
- **Verify:** documentation review; cross-check against the fixtures.

## 10. Deferred and later

- **R-STRAT-RICH (after Plain E2E, Plain keys and a period of real use).** Decide the *form* of the rich protocol, not its existence: rich is kept and must be able to compete with native extensions. Gather numbers: key-to-render latency rich versus Plain plus keys; capabilities lost without the rich path (cursor control, key capture, folds, text mode); lines of code and tests the rich path costs (render/ack/resync, keymap delivery, static bindings, text mode, most of the VSCode adapter); bugs attributable to it. Options: keep both, reduce rich to a core, or move bundled clients onto Plain plus a key shim (VSCode could bind keys straight to `executeCommand`, which needs a document-identity story). Output is a decision document. The earlier "extension audit" is dropped; the standing rule (G5) is that any one real consumer keeps an extension.
- **D-1: Paged and query pick delivery.** Paged = first page plus `hasMore`, client asks for more (browsing only; client fuzzy filter sees loaded pages). Query = client sends typed text (debounced, cancelable), server returns ranked matches (one round trip per keystroke; this is also the server fuzzy picker). Static covers refs; history picks use log select (already paged via `log.load-more`). Build only when a command needs file or commit-search picking. Neovim `vim.ui.select` is static-only.
- **D-2: Magit skip-at-point option** (opt-in setting, default off, optional confirm).
- **D-3: Server-side fuzzy picker** (see D-1 query mode).
- **D-4: Multiple clients on one repository.** Unsupported for now. Investigation ticket: mutation serialization, watcher refresh reaching all clients, stale `ActionTarget` from client B after client A mutates.
- **D-5: R-EDIT-HISTORY** unchanged.
- **D-6: Unproven F5 interception cause** in the VSCode host (carried from `NOTES.md`).
- **D-7: Magit command-by-command target audit.** Not needed; targets are assigned by the principle in H-15 rather than matched to Magit. Revisit only if users report surprising picker types.

## 11. Open VERIFY list

Nobody has confirmed these; each belongs to the ticket named.

- `"//generated"` key tolerated by VS Code and vsce (H-07, H-10).
- CDP `Input.dispatchKeyEvent` triggers contributed keybindings; dialogs are DOM-visible under `window.dialogStyle: custom` (H-07).
- `ctrl+t` and `ctrl+shift+x` workbench collisions (H-07, H-10).
- The `common`-scope, `commit-message`-scope and `magit-commit` contributions are dead (H-10).
- Neovim stale-edit and `documentChanges` behaviour, confirmed by the in-process mini.test (H-14).
- Helix: commit-file attach, goto-definition fallback, uncoloured custom token types, unknown-token rendering (H-14, H-16).
- `scripts/test_plain_neovim.py` contents and whether any of it is reusable (H-17).
- Which stub-only behaviours `integration.js` covers that need porting (H-09).
- That the re-added H-00 test is structural only.

## 12. Suggested first dispatch

Run in parallel: H-00, H-01, H-07, H-10, H-16, H-18 (H-18 after the H-01 doc rules land). Then H-02 and H-03. Then H-04, H-08, H-11. Then H-05, H-09, H-12, H-14. Then H-06, H-15, H-17. Keep H-13 (spec skeleton) growing throughout. Phase F starts after R-PLAIN-GATE.

## 13. Notes for whoever dispatches this list

- **Unconfirmed paths.** Any path marked (confirm) was inferred from evidence reports, not read from the tree. The agent confirms it before editing and corrects the ticket if it differs.
- **Plain keys numbering.** PK-1 to PK-6 replace the nine tickets in `magit-lsp-plain-keys-tasks.md`. Update any reference to the old IDs when merging into TASKS.md.
- **Unread files.** `scripts/test_plain_neovim.py` (H-17) and the contents of `integration.js` beyond the excerpts (H-09) have not been read. Their tickets begin with reading them.
- **Open VERIFY items** are collected in section 11; each belongs to the named ticket and should be settled with text evidence (G1).
- **Pickers.** Targets are assigned by the principle in H-15, not by matching Magit command by command; the earlier Magit research request is dropped (D-7).
- **Re-added H-00 test.** It was simplified after the original was removed and has not been reviewed; H-00 starts there.
- **Format.** This list is meant to be exported to Markdown and merged into TASKS.md in the roadmap ticket template; completed tickets then collapse per G6.
