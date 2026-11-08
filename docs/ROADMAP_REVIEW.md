# Roadmap review at 243ff20

Source review only: reviewed `ebe0516..243ff20` (20 commits, 14 paths, +2501/-146). This is not runtime proof. Reuse recorded Phase 3 gate evidence; do not rerun heavy tests for this review. Live checkboxes and Done entries in `TASKS.md` remain the sole status authority.

## Findings and forward order

1. **P1 — guard asynchronous Plain template delivery.** `src/server.rs:1071–1085` unconditionally resolves Abort after asynchronous failed, rejected, or timed-out template `applyEdit`. Meanwhile exact `didOpen` may already attach the draft, or a newer `EditDocument` for the same run may supersede delivery (`src/editor.rs:380–423`). Implement an atomic registry guard scoped to current delivery identity and unopened state; never check then abort. Permanent regression oracle: late rejection N after exact `didOpen`, or after replacement N+1, does not cancel; rejection of genuinely current unopened delivery cancels with readable message and no commit. Cover both arrival orders and timeout/error equivalents. Verify with existing `make test-rust FILTER="--test plain_draft --test editor_bridge"`. Do not implement in this review.
2. **P2 — deferred sibling refresh.** Preserve existing scope and dependency; do not let it precede the P1 correctness fix.
3. **Plain draft flake diagnosis, then fix.** Current queue text overstates root cause. One edit targeting v2 normally advances client to v3; that observation proves neither a second stale edit nor a rejection. Retain symptoms and `/tmp/opencode/diag_fail.log` artifact, but mark mechanism unproven. Capture correlated run/URI/incarnation, generation, render creation, prepare suppression/in-flight state, outbound request ID/version, response, `didChange` echo, and expiry. Diagnose before production changes or retry masking. Concurrent dirty harness work stays excluded and uncommitted. Ten consecutive passes are supplementary, not regression proof; add deterministic regression after cause is known.
4. **Baseline UI debt.** Keep the five baseline scenarios (`common-navigation`, `hunk-body-fold`, `log-select`, `log-view`, `process-log`) explicit as owned debt or documented gate disposition. Existing `make ui-file` scenarios are real native Neovim; use text/structured evidence only.

## Review record

Verified: `make docs-check` @ `eb3ac8d` → ok. Documentation check only; source observations above are not runtime proof.
Verified: `make docs-check` after the final queue corrections based on `20b28ba` → exit 0 (`ok: docs-check`). No implementation changes or new runtime verification in this review.

## Phase 4 — specification

Execute T011 → T012 → T013. T011 is oversized; split during execution into: method/direction/request-notification and serialization-mode inventory; schema plus Make drift/validation targets; generated TypeScript/Lua plus only shared effects/prompts lists; payload-table generation and fixture checks. Rust serialization is authority; method table remains explicitly maintained. Fixture-cover `Option<Value>`, flatten/custom serialization, and serde-mode hazards. Add new Make targets before invoking them; do not invent target names. Distinguish `PlainActionTarget` wire-shape changes from generator-only changes: update both peers and fixtures atomically, with no compatibility shims.

T012 normalizes volatile paths, IDs, and timing without hiding order or version semantics. Replay real-binary C2S and S2C traffic, including clear interleavings. Add permanent schema/golden tests. T013 skeleton may start independently, but final content depends on T011/T012. Link docs via `make docs-check`; claims cite tests. Explain support levels as pedagogical; actual capability is per method, not one scalar level. Saved-file source intentionally bypasses live version by reading disk: clarify exact-version wording without removing public support.

## Phase 5 — commit selection and rebase

For T014–T019, prove cancel/preflight failure preserves index, worktree, refs, exact commit graph, and unrelated refs/files. Validate stale selected OID and repository identity. Cover conflict, continue, abort, dirty, and unmerged states. Rewriting shared history needs an explicit policy decision: what counts as published, and what confirmation behavior applies. Do not impose blanket confirmation silently.

## Phase 6 — Plain acceptance

T020/T021/T022 require a completed workflow, reproducible real-host smoke, and explicit gate disposition. Local evidence is Linux-only; cross-platform claims require explicit CI followups. Workers must not install arbitrary binaries. Missing required host or Make target needs an explicit blocker/target ticket. Known failures cannot become green through report-only exit status.

## Phase 6A — rich protocol 0.2 gate

The archive settles the gate: T105/T112/T113/T114/T119 gate 0.2; T115/T116/T117 prototypes and T118 stable freeze do not. Repeat that subset in the live phase/release header so file order cannot accidentally block release. Audit and clean up before v1-preview publication; later incompatible changes need explicit version decision, not shims. Prototype feedback precedes stable freeze but does not block release.

## Phases 7–14

Phase 7 is non-gating, but known correctness failures still matter. Cover capability interactions deterministically; run host spike before runner/cases. Phase 8 needs protected publication/credentials, platform build-only versus tested distinctions, clean install/dry-run Make targets (add if absent), partial-publication recovery, checksums, license provenance, and human approval. Concurrent release work remains unreviewed.

Phase 9 is optional after T022 and default-off. Use deterministic seeded bounded sequences, replay trace, and exact Git no-mutation oracle for ordinary typing. Clipboard, IME, format, save, multicursor, and undo patterns must never infer destructive actions.

Refine Phases 10–13 only when scheduled: each ticket needs backend boundary, capability contract, dirty/failure/cancel oracle, and Make target/native evidence. Network/reset/worktree/submodule/arbitrary command and jj operations need trust/safety boundaries and separate tickets. Phase 14 remains explicitly deferred pending owner decision. Broad feature names are not implementation-ready.
