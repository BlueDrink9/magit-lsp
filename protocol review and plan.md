# magit-lsp roadmap: progress review and tidied queue

*Prepared 1 November 2026 from the current TASKS.md and the harness/client-model plan ([earlier task list](https://claude.ai/artifact/A962ngijTNQF4x8A9AsWp3), referred to below as the H-list). This document replaces TASKS.md §7 and lists the edits to the other sections; it does not reproduce unchanged design sections. Ticket bodies for H-nn and PK-n tickets stay in the H-list.*

The Plain foundation is largely built, and release is held up by three things: a red test baseline, the rebase chain, and Plain adapters that still need prompts, selection and commit editing. Section 2 states what I'd decide first.

## 1. Progress review

| Track | Done | Open |
| --- | --- | --- |
| Plain foundation | DOC, HARNESS, PREREQ, SYNC, REPO, RENDER, VIS, TARGET, DIFF-SAFETY, ARGS, 4.1-head-edit (11) | PROMPT, STACK (option prompts only), SELECT, DRAFT, ACTIONS, CORE, E2E, HELIX, SPEC, GATE, EDIT-REAL parent (11; four are umbrellas whose children are already done) |
| Release prerequisites | R-CLIENT-SPEC | SELECT-OPS, 4.1a, EDIT-KINDS, OP-STATE, TODO, 7.3a |
| Release sprint | REL-1a, REL-1b, REL-2a (sub-slices) | REL-1, REL-2, REL-3, REL-4 |
| Review and bugfix queue | everything listed | nothing |
| Harness and client model (H-list) | nothing started | H-00..H-18, PK-1..PK-6 |

### What the record shows

1. **The baseline is red, and the red parts sit under release prerequisites.** Recorded failures, all deferred by "Defer and continue Plain": Rust `errors` (the test you since removed and re-added), three `log_commands` tests, one `views` test, and UI scenarios `log-select`, `log-view`, `process-log`. An earlier Lua clipboard failure (`tests/test_magit_lsp.lua:1249`) is also recorded with unknown status. R-4.1a depends on log select, and R-PLAIN-GATE and R-REL-4 both require a full `make test`. No ticket owns this. H-00 now does (section 3).
2. **Helix, the motivating Plain client, cannot commit as specified.** Its initialize trace declares no `window.showDocument`, and R-PLAIN-DRAFT makes commit commands `Unavailable` without it. R-PLAIN-HELIX's "commit with save and close" cannot pass. This needs a decision (H-14 lists the options) before PLAIN-DRAFT is built.
3. **Stock Neovim is inert on Plain.** By the comment in `tests/ui/plain_init.lua`, stock Neovim omits `workspace.executeCommand`, and the server requires it. Its trace also lacks `documentChanges`. The README claim that generic LSP clients work is untrue until H-14 lands.
4. **Umbrella tickets linger.** ACTIONS, STACK, CORE and EDIT-REAL have every child done. What remains is consumer work that lives in other tickets, so these should close with a pointer, or they stay open forever.
5. **The rebase chain is the long pole.** EDIT-KINDS, OP-STATE, TODO, then 7.3a (which also needs SELECT-OPS, EDIT-REAL parent and Plain acceptance) is four tickets of new state machinery, none started, and it gates the release.
6. **No hosted CI run is recorded.** R-REL-1a says hosted CI is unverified, and §8.7 promises Linux/macOS/Windows. The first hosted run will surface the red tests in finding 1.
7. **TASKS.md is over 2,000 lines.** About 150 lines of §3.10 are dated evidence copies of §7 entries, the Plain bodies appear in three places, and the no-vision policy appears in four.

## 2. Decisions needed from you

1. **Interactive rebase in 0.2?** R-7.3a is the largest unstarted item. Options: keep it as the gate; ship 0.2 with fixup selection (R-4.1a) and move 7.3a to 0.3; or keep both and accept the date risk. I'd decide before anyone starts EDIT-KINDS.
2. **Close the umbrellas now** (ACTIONS, STACK, CORE, EDIT-REAL) with their residuals moved into named tickets. I recommend yes. For EDIT-REAL, list the unverified lifecycle cases (last-window exit, dirty shutdown, cross-build) as an explicit deferred item.
3. **Helix commit rung** (H-14 item 6): message the path and accept any `didOpen` of the announced URI; mark commit `Unavailable` on Helix; manual-open with explicit Confirm and Abort actions; or a goto-definition fallback row. Decide before R-PLAIN-DRAFT.
4. **Which H tickets gate release.** Section 4 proposes a short list; the rest run in parallel or after.
5. **R-4.1-basic** looks already satisfied by R-4.1-head-edit and `commit-menu-shows-actions.scn`. I'd audit and delete it.

## 3. Open work that gates 0.2 (ordered)

Tickets not named here keep their bodies from the TASKS.md you pasted. Changes are in the last column.

| Order | ID | Depends on | Done when (short) | Change from today |
| --- | --- | --- | --- | --- |
| 1 | **H-00 Baseline** | none | `make test` and `make ui-test` are green on a clean tree; the re-added `errors` test reviewed (structure only, no Git wording); each red test above fixed or deleted with a reason; IMM-3 table rows still covered | New scope: the deferred red gates become one ticket |
| 1 | **H-01 Docs diet** | none | See section 5; also cap completed tickets, empty REVIEW\_REPORT after audit, strip "both clients native" from five tickets | No-vision policy and the template's native-evidence rule are already done; drop those items |
| 1 | **H-02 Wire recorder** | none | Env-gated server JSONL recorder with redaction; real initialize and didChange traces for bundled Neovim, bundled VSCode, plain Neovim and Helix | New |
| 2 | **H-03 Typed declaration and purge** | H-00 | Typed `MagitCapabilities`; delete variants with no consumer; camelCase `ActionTarget`; checked emitter | New |
| 2 | **H-14 Plain capability fixes** | H-04, H-05 | Gate on `codeAction` only; Neovim `documentChanges` quirk; echo watchdog; in-process mini.test; Helix commit rung | New; fixes findings 2 and 3 |
| 3 | **R-PLAIN-PROMPT** + **H-15 pick ladder** | H-03 | Document prompts for all four kinds; completion; support table generated; `pick` declaration and fallback ladder; the two Lua prompt bugs fixed | Merge H-15 into PROMPT; Plain discard confirmation lands here (currently `Unavailable`) |
| 3 | **R-PLAIN-SELECT** | PROMPT | Parameterized stable-ID single/multi pick with caller restoration; log select for HEAD-reachable targets, text pick otherwise | Implement through the H-15 pick abstraction, not a second mechanism |
| 4 | **R-PLAIN-DRAFT** | EDIT-REAL, ACTIONS, SYNC | Open Git's file through `showDocument`/fallback; explicit Confirm and Abort code actions; saved-file close versus exact-version confirm | Helix decision applied |
| 4 | **R-PLAIN-STACK** close-out | PROMPT | Option rows open prompts; Back and any-line actions already done | Close with option prompts moved under PROMPT |
| 5 | **R-PLAIN-E2E** | DRAFT, SELECT | Harness slice plus plugin-free Neovim scenarios for status, stage, stale, menu, commit exact message and abort | Fold `scripts/test_plain_neovim.py` into `scripts/ui_lib` as `client: plain`, as this ticket originally specified |
| 5 | **R-PLAIN-HELIX** | DRAFT, H-14 | Manual checklist on Helix 25.07.1 | Acceptance rewritten to match the chosen commit rung |
| 5 | **R-PLAIN-SPEC** = **H-13** | H-11, H-12 | One leveled client spec (levels 0-4) with generated tables and example files | R-PLAIN-SPEC, H-13 and the already-done R-CLIENT-SPEC become one rewrite of `docs/CLIENT_SPEC.md` |
| 6 | **R-PLAIN-GATE** | all above, R-4.1a, R-7.3a | Release slices run through the harness; support table regenerated; full gate green | Full gate is realistic only after H-00 |
| parallel | **R-SELECT-OPS** then **R-4.1a** | R-ARGS, R-5.1 | Selected full OID and captured arguments survive confirm/cancel; exact repo state for fixup target | "Native evidence in both clients" becomes Neovim only (template rule) |
| parallel | **R-EDIT-KINDS** then **R-OP-STATE** then **R-TODO** then **R-7.3a** | R-EDIT-3, SELECT-OPS, EDIT-REAL parent | As recorded | Same wording fix; subject to decision 1 |
| any time | **R-EDIT-REAL parent** | H-00 | Close with residuals listed as deferred (decision 2) | Currently blocked by the deferred red gates and unverified lifecycle cases |
| any time | **R-PLAIN-ACTIONS, R-PLAIN-CORE** | their consumers | Close on PLAIN-GATE with a checklist | Umbrellas |

## 4. Harness and client-model track (the H-list, merged)

"Gates 0.2" means the release needs it; "stabilize" means it improves maintainability and can run in parallel or after.

| ID | Size | Depends on | Gates 0.2? | Revision from the H-list |
| --- | --- | --- | --- | --- |
| H-00 Baseline | M | none | yes | Expanded (section 3) |
| H-01 Docs diet | M | none | yes | Two items done already |
| H-02 Wire recorder | S | H-01 (policy) | yes | Unchanged |
| H-03 Typed declaration and purge | M | H-00 | yes | `ActionTarget` is already in `src/dispatch.rs` with snake\_case fields; rename to camelCase and update Plain tests |
| H-04 Quirks table | M | H-02 | yes (Neovim and Helix rows) | Unchanged |
| H-05 Capability-model fake editor | L | H-02..H-04 | yes | R-PLAIN-HARNESS is done (12 tests, real pump); H-05 extends it: move `ClientPump` out of `plain_harness.rs`, configurable capabilities, rich view model, dynamic ids |
| H-06 Lattice and combination tests | M | H-05, PROMPT | stabilize | Unchanged |
| H-07 Real extension-host spike | S | H-01 | stabilize | Unchanged |
| H-08 E2E runner redo | M | H-07 | stabilize, but required for hosted CI to run VSCode tests | Unchanged |
| H-09 Host cases, seams, delete the fakes | L | H-07, H-08 | stabilize | Unchanged |
| H-10 Keybinding compaction | M | none | stabilize | Verify the dead `common` and `commit-message` blocks first |
| H-11 Schema generation | M | H-03 | stabilize | Unchanged |
| H-12 Example files | S | H-11 | stabilize | Unchanged |
| H-13 Client spec | L | H-11, H-12 | yes (merged with R-PLAIN-SPEC) | Skeleton can start now |
| H-14 Plain capability fixes | M | H-04, H-05 | yes | Unchanged |
| H-15 Pick and prompt ladder | M | PROMPT, H-03 | yes (merged into PROMPT) | Static delivery only |
| H-16 Colour roles | S | H-01 | stabilize | `color_spike` is broken; delete it |
| H-17 Drivers and `.scn` slimming | M | H-05, Plain E2E | stabilize | The `test_plain_neovim.py` fold moves to R-PLAIN-E2E; H-17 keeps single-combo default, conformance set, scenario x driver matrix |
| H-18 Hygiene tooling | S | H-01 | stabilize | Run at phase ends, report-only first |

**Plain keys** (after R-PLAIN-GATE and H-02), all optional for 0.2: PK-1 record edit shapes (gate); PK-2 net-edit extraction and classifier; PK-3 policy, arming and settings (global, per-client, per-document, quirks ceiling); PK-4 execution; PK-5 safety and end-to-end; PK-6 documentation. It is the motivating feature for Helix; Helix commit editing (decision 3) is what blocks the Helix experience first.

## 5. Doc tidy: what H-01 covers and what changes in each section

**Already done, so dropped from H-01:** the no-vision policy (AGENTS.md, `ui-test-plan.md`, NO-VISION-TESTS, `clients/vscode/test/no-vision.js`) and the template's rule that VSCode native evidence is needed only for adapter-capability changes.

**Still to do in H-01:**

1. Collapse every completed ticket to at most 12 lines (requirement, `Done: <commit>`, the test that enforces it). Evidence narratives live in commit messages.
2. Extract the host behaviours that only `clients/vscode/NOTES.md` records (F5 interception, find-widget and QuickPick focus, fold chords) into `docs/evidence/vscode-host-behaviours.md`, each with its Codium version. Carry "F5 interception cause unproven" into a deferred item. Then delete NOTES.md.
3. Audit `REVIEW_REPORT.md` finding by finding (A01..A34), copy any open item into TASKS.md as one line (known candidates: A11 versus EDIT-REAL, BUG-READ-1), then empty it to a header and an empty "Open findings" template.
4. Remove "native evidence in both clients" from R-SELECT-OPS, R-4.1a, R-7.3a, R-OP-STATE and R-TODO (it is still in all five).
5. Move any screenshot code to `scripts/readme-shots/` (human-run `make readme-shots`); rewrite R-REL-2's CI screenshot job as a manual task; mark PNG-citing evidence historical with a one-line banner and leave its contents.
6. Add the golden/snapshot rule to AGENTS.md: separate commit, every changed hunk explained, `UPDATE_EXPECT` used rarely.

**Edits to the other sections of TASKS.md**

| Section | Edit |
| --- | --- |
| Top matter | Delete the completed "Ad hoc" evidence-report item and the review-checkpoint paragraph (move the checkpoint to the archive). Keep the "Last review" hash. |
| §0 decisions | Replace the "Commit selection (target)" row with the picker rule (HEAD-reachable targets use log select; everything else uses a free-text pick with the at-point value preselected; never skip). Add rows for the test policy, the generated-artifact rule, capability resolution with quirks, and "rich is kept; its form is a later decision". Merge the seven Plain "(target)" rows into two; they restate §3.4 and §4.2. |
| §2 contracts | Shorten IMM-1..9; each points to its enforcing test by name, and a missing test is recorded as a gap. IMM-8 and IMM-9 become one line each. |
| §3.6, §3.7 | Add a "Prompt presentation" principle (semantic prompt, per-client presentation, document prompt as universal fallback, `secret` rich-only). Replace "no client picker" with "clients never filter, parse or resolve Git state". Add the `pick` shape and the log-select rule. |
| §3.9 | Delete `forwardAll` if H-03 finds no consumer; describe the generated shape (emit `common` once, then per-scope differences); add the workbench denylist rule. |
| §3.10 | Collapse about 15 dated slice entries into the contract (they duplicate §7). Move the R-EDIT-2 process-ownership mechanism (about 60 lines) to `docs/design/process-ownership.md`. Keep the contract, ring and askpass paragraphs. |
| §3.15, §8.4 | Replace the 28-type table with about 10 roles and standard token types where they fit. Delete the TOHTML plan and IMM-8 spike; keep three colour tests (Rust token response, per-client legend coverage, one ANSI smoke). |
| §4 | After H-11, generate payload tables from the schema; keep the prose. |
| §5.5 | Delete the commit-message scope if H-10 confirms it is dead. |
| §8.7 | State what CI actually runs; hosted CI is unverified. |
| §9 | Record the answers already written inline: macOS config directory uses `dirs` plus a root edit-config command; Escape cancels most things; full-line tint is wanted. |
| §10, §11, §12 | Delete (historical limitations, review pointers, review backlog). The archive holds them. |

**Duplicates removed**

| Duplicated item | Where it appeared | Resolution |
| --- | --- | --- |
| No-vision policy | AGENTS.md, `ui-test-plan.md`, TASKS §7, Plain tasks file | AGENTS.md only |
| R-DIFF-SAFETY | Plain phase list and the shared Git queue | One ledger row (done) |
| R-PLAIN-\* bodies | TASKS.md and the Plain tasks file | TASKS keeps one line and a status; bodies stay in the Plain file |
| R-EDIT-REAL | §3.10 slices, Plain-phase slices, the parent, the Plain file | One ledger row and one contract in §3.10 |
| R-EDIT-3/4/5/6/8 | Historical superseded tickets | One ledger line |
| R-4.1-basic and R-4.1-head-edit | Both specify amend, extend, reword, `-a`, `c c` | Audit, then delete R-4.1-basic |
| R-CLIENT-SPEC (about 110 lines of review sub-tickets), R-PLAIN-SPEC, H-13 | Three entries for one document | One ledger row plus one open ticket |
| R-PLAIN-PROMPT, R-PLAIN-SELECT and H-15 | Three pick/prompt designs | One abstraction |
| R-PLAIN-E2E and H-17 | Both fold `test_plain_neovim.py` into `ui_lib` | E2E owns the fold |
| R-PLAIN-HARNESS and H-05 | Harness described twice | H-05 extends the done ticket |
| BUG-READ-1 | Entry plus mentions in §3.10, §10, §11, §12, R-REL-4 | One entry (minimal slice done, larger design deferred) and one release disclosure |
| Plain keys | Nine tickets in a separate file | PK-1..PK-6 |

## 6. Completed work (collapsed ledger)

| Group | Completed |
| --- | --- |
| Correctness contracts | IMM-1..IMM-9 |
| Review fixes | R-COPY-SOURCE, R-NAV, NO-VISION-TESTS, Plain lifecycle P0 and echo P1 |
| Bugfix and UX | BUG-STATUS-FOLD, BUG-STAGE-NEXT, BUG-STAGED-STAGE, BUG-EDIT-WINDOW, UX-VIM-HORIZONTAL, BUG-READ-1 (minimal slice), repository discovery on Git 2.30, readable non-repository error, Plain EOF repair loop with bounded drift guard |
| Diff safety and targets | R-DIFF-SAFETY (both review corrections), R-PLAIN-TARGET (both review corrections; the identical-content ordinal exception is approved narrowly) |
| Editor bridge | R-EDIT-REAL slices (cutover, native close in both clients, lifecycle, multi-root, rejection and retry, history removal, canonical scenario), R-EDIT-2/3/4/5/6/8 (historical) |
| Commit operations | R-ARGS, R-4.1-head-edit (amend, extend, reword, reset-author), stale commit scenarios |
| Plain foundation | DOC, HARNESS, PREREQ, SYNC, REPO, RENDER, VIS (with its four children) |
| Plain actions (children) | executeCommand, title-style config and rendering, cursor, section and selection actions, unsupported refusal, availability descriptions, latency, native content actions |
| Plain stack (children) | menu open and Back, switches, concrete log, log to commit menu, native menu and log, undo repair |
| Plain core (slices) | capability model, typed delivery seam, typed dispatch, rich feature enforcement (two slices) |
| Release and docs | R-CLIENT-SPEC, R-REL-1a, R-REL-1b, R-REL-2a |

## 7. Release sprint (after R-4.1a, R-7.3a, the client spec and R-PLAIN-GATE)

| ID | Remaining | Notes |
| --- | --- | --- |
| R-REL-1 artifacts | Registry dry runs; dependency, upstream-license and corresponding-source review; clean-machine install; hosted CI run; checksums; publication approval | 1a and 1b are done. Needs H-00 green first so CI is meaningful. |
| R-REL-2 client install | lazy.nvim manager smoke; published-VSIX install; README assets | Screenshots are a manual human task. No CI image job. |
| R-REL-3 README | Features versus Planned; fair Neogit and Edamagit comparison; badges; scoped coverage; the author's disclosure last | Claims must match evidence. |
| R-REL-4 go/no-go | Changelog; safety warnings; platform matrix; full regression; `make hygiene` run (H-18); human go/no-go; tagged publication and live badges | Disclose BUG-READ-1's remaining same-session intake limits. |
