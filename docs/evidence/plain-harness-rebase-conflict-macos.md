# Plain harness rebase-conflict macOS rerun

The plain harness rebase-conflict test
`plain_gate_rebase_conflict_is_readable_and_abort_restores_exact_state`
(`tests/plain_harness.rs`) passed every assertion through the interrupt
(`repo.git(&["status", "--porcelain"])` == `UU f.txt`) on macOS and then failed
in the resolve-and-continue window. The failing window could not name its step:
`plain_gate_run` asserts only that the response carries no JSON-RPC error, and a
skipped stage or a failed `git rebase --continue` both return success with a
message effect.

## Hardening (this change)

- After the "Unstaged changes" `status.stage` action, the test now reads the
  index with `git show :f.txt` (which panics on a nonzero Git exit, so an
  unmerged entry fails here) and asserts porcelain no longer contains
  `UU f.txt`.
- The menu `r` continue response is now asserted to carry no `message` effect,
  matching the failed-versus-successful oracle in `tests/server_async.rs`
  (a failed continue yields one warn message; a successful one yields none).

## Outcome

| Run | Command | Result |
| --- | --- | --- |
| Linux (local) | `make test-rust FILTER="--test plain_harness"` | Pass: 35 passed, 0 failed. |
| macOS | `make test-rust FILTER="--test plain_harness"` | Pending: review-only rerun, not performed in the Linux working session. |

Interpretation for the macOS rerun:

- Green: the production path is covered and the earlier failure was
  environmental or is no longer reproducible at this revision.
- Fails at the new index assertion: `git add -u` skipped the unmerged entry, so
  the stage action is the defect; the follow-up belongs in the production
  staging boundary (`src/git.rs`), not the harness.
- Fails at the new no-message-effect assertion: the continue failed after a
  correctly staged resolution; the warn message effect text names the Git
  failure and the follow-up belongs in the rebase continue action.
