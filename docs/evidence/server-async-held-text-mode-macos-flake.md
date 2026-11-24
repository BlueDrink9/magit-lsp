# Server-async held text-mode macOS flake

Two tests in `tests/server_async.rs` failed on macOS:
`status_open_text_mode_defers_held_completion_until_exit` and
`status_open_text_mode_exit_honors_refresh_after_held_completion`. Both hold the
status `git` read behind `GatedGit`, toggle text mode while the acquisition is in
flight, release the gate and then expect the acquired result to stay unpublished
until text mode is exited (full replacement at `version + 1`). On macOS the
resync during text mode reported the still-loading tree, and the exit toggle
published a loading tree instead of the fixture text written at
`tests/server_async.rs:583` and `:692`.

## Analysis

Every `view/open` arms a live `RepoWatcher`
(`src/server/requests.rs:132-143` → `src/server.rs:583`), which uses the 250 ms
production debounce (`src/watcher.rs:37`) unless
`MAGIT_LSP_WATCH_DEBOUNCE_MS` overrides it (`src/watcher.rs:50-58`). Neither
test pinned that variable before this change, so a watcher burst could land in
the held-completion window.

The chain, verified in the source:

1. A watcher burst routes through `refresh` → `refresh_view`
   (`src/session.rs:2568-2592`). While the status view is still `Loading`,
   `queue_refresh_while_loading` records a pending refresh and bumps the
   generation (`src/status_view.rs:210-219`).
2. The released completion finds that pending refresh and returns
   `InstallStatus::Coalesced` (`src/server/view_execution.rs:337-339`) instead of
   installing. A successor acquisition is spawned
   (`src/server/view_execution.rs:280-291`), so the first result is discarded and
   the view remains `Loading`.
3. The completion barrier still passes: `status_completion_processed` is traced
   for every outcome (`src/server/view_execution.rs:305-311`), so the test
   cannot distinguish installed from coalesced.
4. The resync therefore honestly reports the loading text, and
   `leave_text_mode` (`src/session.rs:372-384`) publishes whatever is stored —
   the still-loading tree — at `version + 1`. That is both reported failures.

The interfering burst itself is INFERRED, not verified: macOS FSEvents late
delivery/coalescing, or a pathless must-scan rescan notice
(`src/watcher.rs:161-163`), is the plausible source, matching the standing
hazard recorded in `docs/evidence/plain-fresh-bulk-macos-flake.md`. The status
read writes nothing (`--no-optional-locks`, `src/git.rs:2285-2288`) and writes
made before the watcher is armed are never delivered
(`docs/evidence/plain-render-watcher-timeout-macos.md`). Linux inotify fires no
event in this window, which is why the suite is green there.

No production bug is claimed: coalescing is the designed response to a real
burst. The tests simply did not account for the armed watcher on macOS.

## Hardening (this change)

- Both tests now push `("MAGIT_LSP_WATCH_DEBOUNCE_MS", "60000")`
  (`tests/server_async.rs:596` and `:705`) into the spawned server's environment,
  following the `60000` precedent at `tests/plain_actions.rs:1439`. The value
  reaches the server through `TestServer::spawn`'s `extra_env`
  (`tests/common/stdio.rs:101-116`).
- The long debounce cannot stall shutdown: dropping the watcher closes the
  channel, the thread exits on `Disconnected` (`src/watcher.rs:214`), and
  `join`/`Drop` release the retained sender (`src/watcher.rs:231-247`).
- No assertion, gate mechanic or two-phase loading expectation changed. Other
  pins in the file are untouched: the watcher-positive tests keep `25`
  (`tests/server_async.rs:351`) and `50` (`:469`), and the coalescing test
  relies on its own real burst.

No production code changed.

## Outcome

| Run | Command | Result |
| --- | --- | --- |
| Linux (local) | `make test-rust FILTER="--test server_async"` | Pass: 66 passed, 0 failed. |
| macOS | `make test-rust FILTER="--test server_async"` | Pending: review-only rerun, not performed in the Linux working session. |

Interpretation for the macOS rerun:

- Green: the failure came from the unpinned 250 ms watcher debounce, as analyzed.
- Still red at the resync/exit assertions: a burst is not the only interference;
  inspect `status_completion_processed` traces for a `Coalesced` outcome and
  treat the remaining path as a production question, not a test tweak.
