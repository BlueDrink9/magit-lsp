# Task queue migration ledger

Scope: old `TASKS.md` §7 unchecked items, both Plain planning files, and review
findings A01–A34. Status follows explicit checked records and evidence in the
source documents; proposal text alone is not closure evidence.

## Review findings

| Finding | Status / replacement |
| --- | --- |
| A01–A10 | Complete per `REVIEW_REPORT.md` reconciliation; retained historical, no new ticket. |
| A11 | Complete Linux cutover, acceptance owner R-EDIT-6; historical managed-design adversarial acceptance. R-EDIT-REAL residual lifecycle cases T104; platform gates T100/T101. |
| A12 | Open, retained gap: no numbered replacement ticket found. Preserve sole issue record; needs owner assignment before implementation. |
| A13–A21 | Complete per checked records/reconciliation; no new ticket. |
| A22–A23 | Withdrawn, no ticket. |
| A24–A34 | Complete per checked records/reconciliation; no new ticket. |

BUG-READ-1: minimal intake/unblocked-request slice is complete under R-PLAIN-PREREQ;
evidence is the checked record and probe noted in the Plain task file. The larger
same-session read redesign is explicitly deferred to T103. It is not closure of
the larger design. A11 is not assigned to BUG-READ-1: its acceptance owner is
R-EDIT-6, and residual lifecycle acceptance is T104.

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

- A12 has no numbered replacement or assigned implementation owner in the
  supplied queue; retained gap.
- R-EDIT-REAL parent acceptance is not green. T104 covers only listed residual
  lifecycle cases; preserve the broader gates and historical failures.
- BUG-READ-1's larger redesign is intentionally deferred to T103.
- The replacement index does not provide per-finding test evidence for each
  A01–A10/A13–A21/A24–A34. Their completed status is based on explicit
  reconciliation, not independently re-audited here.
