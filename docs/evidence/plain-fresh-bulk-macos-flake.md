# Plain fresh-bulk macOS flake

The plain test `section_untracked_stage_and_staged_unstage_are_fresh_bulk`
(`tests/plain_actions.rs`) failed on macOS at
`repo.git(&["ls-files"]) == "f.txt\n"`: the index still held the staged
`new.txt`/`later.txt` after the offered section `status.unstage`. Nothing in the
failing window could name its step: `execute_offered` returns the raw response,
and every refusal between offer and mutation — plain stale
(`src/server/plain_actions.rs`), session stale (`src/session.rs`), changed
content or nothing to unstage (`src/commands.rs`) and Git failure
(`src/dispatch.rs`) — is a JSON-RPC success carrying only a `message` effect.
The test's `result["error"].is_null()` guard therefore cannot see a skipped
unstage, and the mismatch surfaced opaquely at `ls-files`.

## Analysis

Section offers are content-free by design (`src/dispatch.rs`), so the
out-of-band `git add f.txt` before the unstage does not itself invalidate the
offer. The surviving hazard is the generation gate: the plain bootstrap arms a
live `RepoWatcher` (`src/server/plain.rs`) and this test did not pin
`MAGIT_LSP_WATCH_DEBOUNCE_MS`, leaving the 250 ms production default
(`src/watcher.rs`). macOS FSEvents delivery/coalescing (unlike Linux inotify)
can land a watcher refresh inside the offer-to-execute window, bump the plain
generation and get the offered unstage refused as stale. The alternative, a
failing `git reset`, reports a message effect too, so both readings were
silenced identically. Case-insensitivity and ordering are ruled out: the
assertion compares exact sorted `git ls-files` output over lowercase ASCII
paths, and the observed and expected values differ by content, not order.

## Hardening (this change)

- The test now spawns its server with a pinned
  `MAGIT_LSP_WATCH_DEBOUNCE_MS` (`60000`) through
  `open_plain_with_env`, which reuses `open_plain`'s exact capabilities. The
  long debounce cannot stall shutdown: dropping the watcher's event source
  disconnects its channel and the thread exits promptly (`src/watcher.rs`).
- Both `execute_offered` responses assert no `message` effect, matching the
  failed-versus-successful oracle in `tests/plain_harness.rs`. Any silent
  refusal or Git failure now fails at the refused command with its message
  text, not later at `ls-files`.
- All Git-state assertions and expected values are unchanged, so the test
  still proves fresh-bulk semantics: stage-after-offer stages the later
  untracked file too, and unstage-after-offer resets the whole index to HEAD.

No production code changed: no defect was verified, and section-bulk freshness
semantics are correct and already covered.

## Outcome

| Run | Command | Result |
| --- | --- | --- |
| Linux (local) | `make test-rust FILTER="--test plain_actions"` | Pass: 29 passed, 0 failed. |
| macOS | `make test-rust FILTER="--test plain_actions"` | Pending: review-only rerun, not performed in the Linux working session. |

Interpretation for the macOS rerun:

- Green: the earlier failure was environmental or is no longer reproducible at
  this revision.
- Fails at a no-message-effect assertion: a refusal or Git failure skipped the
  bulk mutation, and the message effect text names the refusing gate (stale
  generation, hunk changed, nothing to unstage, or the Git error).
- Fails with a `message` effect absent but the index still stale: the refusal
  path is silent and the follow-up belongs in the production bulk boundary,
  not the test.
