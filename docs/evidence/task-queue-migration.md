# Task queue migration ledger

Scope: old `TASKS.md` §7 unchecked items, both Plain planning files, and review
findings A01–A34. Status follows explicit checked records and evidence in the
source documents; proposal text alone is not closure evidence.

## Review findings

| Finding | Status / replacement |
| --- | --- |
| A01 | Complete; `da4d422^:REVIEW_REPORT.md` finding A01, acceptance evidence 2026-10-27; security bridge tests and full gate. |
| A02 | Complete; `da4d422^:REVIEW_REPORT.md` finding A02 and its acceptance evidence; positional-version implementation/test record. |
| A03 | Complete; `da4d422^:REVIEW_REPORT.md` finding A03 and its acceptance evidence; displayed-identity selection record. |
| A04 | Complete; `da4d422^:REVIEW_REPORT.md` finding A04 and its acceptance evidence; close-notification integration record. |
| A05 | Complete; `da4d422^:REVIEW_REPORT.md` finding A05 and its acceptance evidence; prompt/session-lock regression record. |
| A06 | Complete; `da4d422^:REVIEW_REPORT.md` finding A06 and its acceptance evidence; VSCode quit-effect record. |
| A07 | Complete; `da4d422^:REVIEW_REPORT.md` finding A07 and its acceptance evidence; edit-key/focus removal record. |
| A08 | Complete; `da4d422^:REVIEW_REPORT.md` finding A08 and its acceptance evidence; originating-client prompt record. |
| A09 | Complete; `da4d422^:REVIEW_REPORT.md` finding A09 and its acceptance evidence; process-log token record. |
| A10 | Complete; `da4d422^:REVIEW_REPORT.md` finding A10 and its acceptance evidence; repository configuration record. |
| A11 | Managed-draft cutover complete; owner R-EDIT-6; residual R-EDIT-REAL lifecycle cases T104, platform gates T100/T101. See `da4d422^:REVIEW_REPORT.md` A11 and TASKS R-EDIT-6 evidence; not acceptance of these residuals. |
| A12 | Open retained gap: finding is real-dispatch Git-failure handling and removal of `server::mutation` (`da4d422^:REVIEW_REPORT.md` A12). No numbered replacement owner evidenced; do not map to unrelated T103 reader-lock work. |
| A13 | Complete; `da4d422^:REVIEW_REPORT.md` finding A13 and acceptance evidence. |
| A14 | Complete; `da4d422^:REVIEW_REPORT.md` finding A14 and acceptance evidence. |
| A15 | Complete; `da4d422^:REVIEW_REPORT.md` finding A15 and acceptance evidence. |
| A16 | Complete; `da4d422^:REVIEW_REPORT.md` finding A16 and acceptance evidence. |
| A17 | Complete; `da4d422^:REVIEW_REPORT.md` finding A17 and acceptance evidence. |
| A18 | Complete; `da4d422^:REVIEW_REPORT.md` finding A18 and acceptance evidence. |
| A19 | Complete; `da4d422^:REVIEW_REPORT.md` finding A19 and acceptance evidence. |
| A20 | Complete; `da4d422^:REVIEW_REPORT.md` finding A20 and acceptance evidence. |
| A21 | Complete; `da4d422^:REVIEW_REPORT.md` finding A21 and acceptance evidence. |
| A22 | Withdrawn after deeper inspection; disposition recorded in `da4d422^:REVIEW_REPORT.md` reconciliation; no replacement ticket. |
| A23 | Withdrawn after deeper inspection; disposition recorded in `da4d422^:REVIEW_REPORT.md` reconciliation; no replacement ticket. |
| A24 | Complete; `da4d422^:REVIEW_REPORT.md` finding A24 and acceptance evidence. |
| A25 | Complete; `da4d422^:REVIEW_REPORT.md` finding A25 and acceptance evidence. |
| A26 | Complete; `da4d422^:REVIEW_REPORT.md` finding A26 and acceptance evidence. |
| A27 | Complete; `da4d422^:REVIEW_REPORT.md` finding A27 and acceptance evidence. |
| A28 | Complete; `da4d422^:REVIEW_REPORT.md` finding A28 and acceptance evidence; retained scaffolding has documented consumers. |
| A29 | Complete; `da4d422^:REVIEW_REPORT.md` finding A29 and acceptance evidence. |
| A30 | Complete; `da4d422^:REVIEW_REPORT.md` finding A30 and acceptance evidence. |
| A31 | Complete; `da4d422^:REVIEW_REPORT.md` finding A31 and acceptance evidence. |
| A32 | Complete; `da4d422^:REVIEW_REPORT.md` finding A32 and acceptance evidence. |
| A33 | Complete; `da4d422^:REVIEW_REPORT.md` finding A33 and acceptance evidence. |
| A34 | Complete; `da4d422^:REVIEW_REPORT.md` finding A34 and acceptance evidence. |

BUG-READ-1: minimal intake/unblocked-request slice is complete under R-PLAIN-PREREQ;
evidence is the checked record and probe noted in the Plain task file. The larger
same-session read redesign remains deferred to T103. It is not closure of the
larger design. A11 is owned by R-EDIT-6, not BUG-READ-1; residual lifecycle
acceptance is T104.

## Open old §7 records

| Old record | Replacement / disposition |
| --- | --- |
| R-PLAIN-PROMPT | T008 |
| R-PLAIN-STACK | T008 option rows; remaining stack umbrella closed |
| R-PLAIN-SELECT | T009 |
| R-PLAIN-DRAFT | T010 |
| R-PLAIN-ACTIONS | Closed umbrella; remaining code-action integration is represented by T007/T009/T010/T022. |
| R-PLAIN-CORE | Closed umbrella; remaining work represented by T009/T010/T022. |
| R-PLAIN-E2E / HELIX / SPEC / GATE | T020 / T021 / T013 / T022 |
| R-SELECT-OPS / R-4.1a | T014 / T015 |
| R-EDIT-KINDS / R-OP-STATE / R-TODO / R-7.3a | T016 / T017 / T018 / T019 |
| R-EDIT-REAL | T104 residuals; parent acceptance remains explicitly not green. |
| R-EDIT-7 / R-EDIT-9 | T100 / T101 |
| R-REL-1 / 2 / 3 / 4 | T030 / T031 / T032 / T033 |
| R-4.1-basic | No new ticket; covered by R-4.1-head-edit and `commit-menu-shows-actions.scn` per queue map. |
| R-4.1-signing; R-5.3a; R-6.3a; R-6.1a; R-8.2a; R-9.1a; R-9.5a | T040–T046 respectively (R-9.1a=T045, R-9.5a=T046). |
| R-VIEW-DATA; R-5.2a/b/c; R-5.3b/c/d/e; R-5.4; R-5.6 | T048–T057 per old-number map. |
| R-5.5; R-BASELINE; R-4.3; R-6.4; R-4.2a/b/c; R-4.1c/d; R-6.1b; R-NETWORK; R-6.2; R-6.3b/c; R-6.5a/b | T058–T073 per old-number map. |
| R-7.1a/b; R-7.2a/b; R-7.3-options/b/c; R-7.4; R-8.1a/b; R-8.2b/c/d/e/f/g; R-9.3a; R-8.3a/b; R-9.1b/c; R-9.2a/b; R-9.4; R-9.5b | T074–T098 per old-number map. |
| R-EDIT-HISTORY; R-SELECT-AUDIT; larger BUG-READ-1; R-STRAT-RICH | T099 / T102 / T103 / T110. |

The above mapping comes from the replacement queue's own old-number index. Any
old unchecked §7 item absent from the table is a retained gap pending exact
ownership and mapping; do not infer it closed. A11/BUG-READ-1 ownership has
been resolved as stated above.

## Plain planning files

The open Plain requirements map as follows: prerequisite/capability work T004–T007;
prompt/pick T008; stable selection T009; provider-file commit editing T010;
specification T013; native Plain vertical slice T020; Helix smoke T021; release
gate T022. Existing completed records R-PLAIN-DOC, HARNESS, PREREQ, SYNC, REPO,
RENDER, VIS and the completed R-PLAIN-CORE/STACK/ACTIONS umbrellas remain history,
not new work. The Plain keys plan maps PK-1 through PK-6 to T034–T039. No extra
open requirement is evidenced by the planning-file completion notes; if later
source audit identifies one, retain it as an unmapped gap rather than treating
this statement as evidence of implementation.

## Retained gaps and evidence limits

- A12 retains its exact real-dispatch Git-failure scope; no numbered owner is
  evidenced. T103 is the separate deferred reader-lock redesign.
- R-EDIT-REAL parent acceptance is not green. T104 covers only listed residual
  lifecycle cases; preserve the broader gates and historical failures.
- BUG-READ-1's larger redesign is intentionally deferred to T103.
- The replacement index does not provide per-finding test evidence for each
  A01–A10/A13–A21/A24–A34. Their completed status is based on explicit
  reconciliation, not independently re-audited here.

## Reconciled decisions

- R-NAV: verified complete 2026-10-28; 74 Rust tests and `common-navigation.scn` passed. The retained navigation decision is reconciled; immutable evidence: `da4d422:TASKS.md`.

## Numbered-ticket immutable provenance

For every numbered ticket below, the exact old scope and its `Verify` text
remain recoverable in immutable `d04dfa2:TASKS.md`; use the old unique ID to
locate the complete record. This is scope provenance, not evidence of completion.

| Numbered replacement | Exact old unique ID | Immutable source |
| --- | --- | --- |
| T040–T046 | R-4.1-signing; R-5.3a; R-6.3a; R-6.1a; R-8.2a; R-9.1a; R-9.5a, respectively | `d04dfa2:TASKS.md` |
| T047 | Section 9 config-edit request; no old ticket ID | `d04dfa2:TASKS.md` |
| T048–T057 | R-VIEW-DATA; R-5.2a; R-5.2b; R-5.2c; R-5.3b; R-5.3c; R-5.3d; R-5.3e; R-5.4; R-5.6, respectively | `d04dfa2:TASKS.md` |
| T058–T073 | R-5.5; R-BASELINE; R-4.3; R-6.4; R-4.2a; R-4.2b; R-4.2c; R-4.1c; R-4.1d; R-6.1b; R-NETWORK; R-6.2; R-6.3b; R-6.3c; R-6.5a; R-6.5b, respectively | `d04dfa2:TASKS.md` |
| T074 | R-7.1a | `d04dfa2:TASKS.md` |
| T075 | R-7.1b | `d04dfa2:TASKS.md` |
| T076 | R-7.2a | `d04dfa2:TASKS.md` |
| T077 | R-7.2b | `d04dfa2:TASKS.md` |
| T078 | R-7.3-options | `d04dfa2:TASKS.md` |
| T079 | R-7.3b | `d04dfa2:TASKS.md` |
| T080 | R-7.3c | `d04dfa2:TASKS.md` |
| T081 | R-7.4 | `d04dfa2:TASKS.md` |
| T082 | R-8.1a | `d04dfa2:TASKS.md` |
| T083 | R-8.1b | `d04dfa2:TASKS.md` |
| T084 | R-8.2b | `d04dfa2:TASKS.md` |
| T085 | R-8.2c | `d04dfa2:TASKS.md` |
| T086 | R-8.2d | `d04dfa2:TASKS.md` |
| T087 | R-8.2e | `d04dfa2:TASKS.md` |
| T088 | R-9.3a | `d04dfa2:TASKS.md` |
| T089 | R-8.2f | `d04dfa2:TASKS.md` |
| T090 | R-8.2g | `d04dfa2:TASKS.md` |
| T091 | R-8.3a | `d04dfa2:TASKS.md` |
| T092 | R-8.3b | `d04dfa2:TASKS.md` |
| T093 | R-9.1b | `d04dfa2:TASKS.md` |
| T094 | R-9.1c | `d04dfa2:TASKS.md` |
| T095 | R-9.2a | `d04dfa2:TASKS.md` |
| T096 | R-9.2b | `d04dfa2:TASKS.md` |
| T097 | R-9.4 | `d04dfa2:TASKS.md` |
| T098 | R-9.5b | `d04dfa2:TASKS.md` |
| T099 | R-EDIT-HISTORY | `d04dfa2:TASKS.md` |
| T100–T103 | R-EDIT-7; R-EDIT-9; R-SELECT-AUDIT; larger BUG-READ-1, respectively | `d04dfa2:TASKS.md` |
| T104 | R-EDIT-REAL residual lifecycle cases | `d04dfa2:TASKS.md` |
| T105 | VS Code F5 open question | `d04dfa2:TASKS.md` |
| T106–T109 | No old record; new tickets | No old record |
| T110 | R-STRAT-RICH | `d04dfa2:TASKS.md` |
| T111 | Section 9 open design questions; no old ticket ID | `d04dfa2:TASKS.md` |

R-EDIT-REAL residuals remain not-green: last-window exit, dirty shutdown,
cross-build, wider reload/workspace changes, platform coverage and broad
failed gates. T104 names only its lifecycle subset. A12 remains real-dispatch
Git-failure handling and removal of `server::mutation`; it has no numbered
owner and is not T103's reader-lock redesign. Future section layout maps to
T093 only if that server-policy scope is correct; whole-file-only binary
updates remain an explicit residual with no owner. Keep §9 dirs/T047 and the
Escape caveat; this note does not rewrite either requirement.
