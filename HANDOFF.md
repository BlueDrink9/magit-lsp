# Session handoff

## Stop point

The user requested a checkpoint and handoff, not further implementation. The
checkpoint intentionally preserves a failing regression; it is not a completed
feature. TASKS.md remains the authoritative completion record.

T018.1, T018.2 and T018.3a are complete. T018.3b1a is the immediate continuation;
its parent and other unfinished children remain unchecked. The original scope is
TASKS_PART_SEVEN.md, T018. Initial interactive-rebase launch belongs to T019,
not this paused-rebase editor.

## Checkpoint changes

- `src/view.rs`: `ViewKind::RebaseTodo`, wire name `rebase-todo`.
- `src/rebase_todo_view.rs`: implement `View`, capture input, scope `rebase-todo`.
- `src/session.rs`: factory constructs the existing paused-todo model.
- `src/commands.rs`: `rebase.edit-todo` opens the view and reports load failures
  readably; `common.cancel` now requests pop for this kind.
- `src/menus/rebase.rs`, `tests/data/magit-keys.txt`: implemented menu action `T`.
- `tests/rebase_todo.rs`: real paused-rebase session regression opens with `r T`,
  checks full commit OIDs, then attempts Escape and repository-preservation checks.

## Known failure and next investigation

Last verification:

```sh
make test-rust FILTER="--test rebase_todo --test commands --test menus"
```

Exit 2. Commands and menus passed; the new paused-todo regression failed after
Escape: expected focused view `v1`, got `v1:rebase-todo`. Latest recorded log:
`build/test-logs/rust.i34Ugo`. No passing cancellation evidence exists. Assertions
after the focus failure have not executed successfully.

Opening through `r T` works and structured row OIDs match the loaded todo. An
earlier test mistakenly used `p T`; that is fixed. `TestSession::new` accepts an
arbitrary repository path; `open_status_view(STATUS_VIEWPORT)` works with the
existing TempDir fixture. No RepoFixture conversion is needed.

`common.cancel` in `src/commands.rs` previously popped only menus and commit
selections. Adding RebaseTodo to that pop condition did not fix Escape. Investigate
key dispatch/default bindings for scope `rebase-todo`, then verify that Escape
actually reaches `common.cancel`. Do not assume the handler is invoked. A scout
claim about a `close_on_cancel` hook was not corroborated by source and must not
be relied on. Keep cancellation on the existing command/stack path.

After this regression passes, finish idle-open error coverage (T018.3b1b), then
identity-based edit/reorder/submit dispatch (T018.3b2), and native conformance
(T018.4). Restore caller and preserve exact todo/HEAD/index on cancel. Use
server tree identity, never rendered-text parsing. The standalone model already
provides `load`, `render`, `set_action`, `move_before`, and guarded `save`.

## Protected work and verification discipline

Do not edit, stage, commit or restore unrelated working-tree changes:

- `AGENTS.md` (pre-existing user rewrite).
- `Makefile` (currently dirty unrelated release/test-target changes).
- Untracked `.github/workflows/release.yml`, `docs/CLIENT_INSTALL.md`,
  `scripts/ci-local.sh`, `scripts/install.sh`, `scripts/package-release.sh`.

Other sessions have committed concurrently. Pre-checkpoint HEAD was `5f6539d`;
inspect current status/log before starting, and do not revert their work.
Task source files above and this handoff/queue note are the authorized checkpoint.

All verification must use Make targets, serially, with no competing builds.
No vision, images or screenshots. Do not weaken the regression or mark the
feature complete merely because earlier tests passed. Generated-reference
changes must be generated through Make and committed separately with causal
justification; the new command may cause later reference/example drift, not yet
verified here. Do not run broad gates merely for this checkpoint.
