# Plain render watcher timeout (macOS)

The plain test `silent_stale_success_without_echo_recovers_after_watcher_render`
(`tests/plain_render.rs`) timed out on macOS with `no response from server` and
no indication of which of its four waits hung or how far the watcher got. The
failure's last sent frame matched the first trigger write exactly: the test
acked `magit-lsp/outbound/1` and immediately wrote `watcher-recovery.txt`
(`tests/plain_render.rs`, before this change), while the watcher arms on a
deferred `spawn_blocking` after didOpen (`src/server/plain.rs`). A write that
lands before the arm is never delivered, and nothing backfills pre-arm events
(`src/watcher.rs` reacts only to stream events); `refresh_all` emits nothing for
an unchanged tree (`src/session.rs`), so one lost event means permanent silence.
Debounce is pinned to 25 ms in this test, so debounce cannot explain a 10 s hang.

## Analysis

Two separate gaps produced an opaque timeout:

- Test ordering: nothing proved the watcher was armed before the trigger write.
  `phase_watcher_setup` trace events exist (`src/runner.rs`) but go to a trace
  file only under an active trace id, never to stderr, so the test could not use
  them as a gate.
- Harness diagnosability: `TestServer::recv_raw`'s timeout panic reported the
  last sent frame, notifications, exit status and a stderr tail
  (`tests/common/stdio.rs`), but not the awaited step, the configured wait, or
  any history of frames already received; `next_edit` consumed frames without
  recording them and the panic pointed at the helper body.

## Hardening (this change)

No production watcher behavior changed: three log lines only, all at
`TRACE`/`DEBUG` under the test's trace filter.

- `src/server.rs`: `repository watcher armed` debug after `watchers.insert`,
  carrying the repository path.
- `src/watcher.rs`: `watcher raw event` trace (kind and paths) before the
  relevance filter, and `watcher burst fired` debug (batch paths) where the
  debounced refresh is emitted.
- `tests/plain_render.rs`: the watcher test polls captured stderr with
  `wait_for_log_line` for the arm marker before writing `watcher-recovery.txt`,
  mirroring `editor_bridge`'s bridge-socket poll, then asserts one raw-event line
  and one burst line naming `watcher-recovery.txt`. The oracle is unchanged: the
  real watcher, without resync polling, still drives both recovery renders; the
  gate only orders the write after arming and stays one test because the halves
  interleave by design.
- `tests/plain_render.rs`: `next_edit` is `#[track_caller]` and labels each wait
  (`next_edit_after`), so a timeout panic names the awaited step, e.g.
  `workspace/applyEdit after writing watcher-recovery.txt`.
- `tests/common/stdio.rs`: `recv_raw`'s timeout panic now reports `within
  {recv_timeout}`, `awaited step: …`, and `received frames (oldest first)` from a
  bounded, 8-entry method-and-id history; the label clears when a receive
  completes. New test `timeout_panic_names_awaited_step_wait_and_received_frames`
  drives that path against a real spawned, silent server and asserts all three
  fields appear in the panic payload.

## Failure-signature map

Read the next macOS failure against the captured stderr (the panic prints its
tail; the assertions name their marker):

| Captured state | Reading | Follow-up boundary |
| --- | --- | --- |
| No `repository watcher armed` before the gate deadline | The arm never completed: the deferred blocking task did not run to the insert, or `ensure_watcher` bailed (`already_armed`/`skipped`) or failed to start (a `file watching unavailable` message effect) | Watcher setup in `src/server.rs`/`src/server/plain.rs` |
| Armed, no `watcher raw event` naming `watcher-recovery.txt` | The write was never delivered to the server: macOS FSEvents delivery/coalescing, the same suspicion recorded in `plain-fresh-bulk-macos-flake.md` | Watcher backend/platform; not reproducible on Linux inotify |
| Raw event, no `watcher burst fired` | The event was filtered out (`relevant_event_excluding`, e.g. a presentation-file path) or the debounce loop kept retrying the index lock (`Refresh::IndexLocked`) | Relevance filter or lock retry in `src/watcher.rs` |
| Burst fired, no render | The refresh saw an unchanged tree and emitted nothing, or the render/applyEdit was lost after arming | `refresh_all` in `src/session.rs`; the render ack path |
| Panic names a later awaited step (`sending the newer client snapshot`, `writing watcher-ready.txt`) | The first watcher render arrived; the hang is in the stale-success absorption half, not arming or delivery | The labelled wait in `tests/plain_render.rs` |

A post-arm baseline refresh would close the remaining write-before-arm window,
but it changes production watcher behavior and is out of scope for this change;
it is recorded here for the macOS rerun decision, not applied.

## Outcome

| Run | Command | Result |
| --- | --- | --- |
| Linux (local) | `make test-rust FILTER="--test plain_render"` | Pass: 22 passed, 0 failed. |
| macOS | `make test-rust FILTER="--test plain_render"` | Pending: review-only rerun, not performed in the Linux working session. |

Check evidence (`A3`): the suite run above is green with the watcher test's
assertions that its captured stderr contains a line with `repository watcher
armed`, a line with both `watcher raw event` and `watcher-recovery.txt`, and a
line with both `watcher burst fired` and `watcher-recovery.txt`; any missing
marker fails the test with the whole captured log after a 20 s poll.

Interpretation for the macOS rerun:

- Green: the timeout was the write-before-arm ordering (now gated) or is no
  longer reproducible at this revision.
- Fails at the arm gate: the deferred arm did not complete; the panic prints the
  captured log, so the `phase_watcher_setup` failure path decides between a
  blocked recursive walk and a refused backend watch.
- Fails at a labelled `next_edit_after` wait: the panic names the half and the
  received-frame tail, and the map above separates delivery, filtering, burst
  and render loss without further instrumentation.
