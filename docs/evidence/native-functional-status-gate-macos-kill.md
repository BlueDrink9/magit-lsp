# Native functional lane: status gate never entered (run 45759764509)

The consumer lane of the release workflow (`native-consumers` in
`.github/workflows/release.yml` :415-426) runs the `server_async` suite against
the downloaded candidate binary. In run **45759764509** the gated test
`status_open_refreshes_while_loading_are_coalesced`
(`tests/server_async.rs`) failed as a silent client-side timeout: the fake Git's
invocation log held exactly
`rev-parse` and `--version`, and the gate was never entered within the 60 s
`ENTERED_TIMEOUT` bound (`crates/test-support/src/fake_git.rs`).

## Diagnosis

- VERIFIED — the release profile is not the cause: `Cargo.toml` has no
  `[profile.release]` section, and the only `debug_assertions` gate in `src/` is
  the effects check (`src/commands.rs:268-273`), which no acquisition path
  consults.
- VERIFIED — the same release binary passed `--test server_async` in the build
  lane (`.github/workflows/release.yml:239-244`), and the consumer lane
  re-verifies the artifact's SHA-256 before and after the run (:421-422), so the
  failure is not deterministic release behaviour.
- VERIFIED — the logged `--version` can only be the below-recommendation probe
  (`src/git.rs:3303-3311`, the sole `git --version` caller in `src/`). It fires
  only after a **reaped** Git child completed with failure
  (`src/git.rs:3149-3152`); a spawn failure returns earlier (`:3134-3135`) and
  runs no probe.
- VERIFIED — the only Git invocation after a status open is the deferred read
  `--no-optional-locks status --porcelain=v2 -z --branch`
  (`src/git.rs:2326-2332`), reached from `src/server/requests.rs:99-104`
  through `src/server/view_execution.rs:157-165` and `:216-221`. The fake logs
  every subcommand before it gates, so a status child that reached the gate
  would appear as `status` in the log.
- INFERRED — the final step: that status child was spawned and died by signal
  before its log echo, in the consumer lane's documented host-kill class (the
  OOM/code-signing collector at `.github/workflows/release.yml:447-471` exists
  for exactly this). The server then published
  "Unable to load repository status" (the same terminal failure the sibling
  test asserts at `tests/server_async.rs:1841-1847`), which the test never saw
  because it was blocked in the silent 60 s `gate.wait_entered()`.

No server change is warranted: the acquisition path behaved correctly for the
Git child it was given; the test's wait was the part that could not report it.

## Reproduction and test hardening (this change)

- `GatedGit::arm_killing` adds a kill mode: the first gated invocation writes
  the `claimed` marker, then kills its own shell with `SIGKILL` **before** its
  invocation-log echo. It therefore never enters the gate and never appears in
  the log — the exact CI signature, produced deterministically. The mode is
  single-use: the next invocation runs the real Git.
- `GatedGit::has_entered` is a non-panicking marker poll, and
  `GatedGit::invocation_log` exposes the recorded subcommands for diagnostics.
- `TestServer::try_recv_within` is a timed, non-panicking peek at the client
  stream (`crates/test-support/src/stdio.rs`); framing errors and a closed
  stream still panic, only silence is benign.
- `wait_for_gate_or_failure` (`tests/server_async.rs`) polls the gate marker and
  the stream together and resolves at the first of: `Entered`, a render of the
  view carrying "Unable to load repository status" (returned with its text), or
  the unchanged 60 s bound, whose panic now carries the fake Git's invocation
  log. The coalesced test asserts `Entered` and keeps its original
  coalescing/ordering assertions unchanged.
- `status_open_refresh_wait_reports_killed_git_child_as_acquisition_failure`
  drives the kill mode and asserts the wait resolves as an acquisition failure
  promptly with that render, that the child never entered the gate, that its
  subcommand is absent from the log, and that only the `--version` probe
  survived it.
- Fixture unit tests in `crates/test-support/src/fake_git.rs` assert the killed
  child dies by signal (`ExitStatusExt::signal() == Some(9)`), that it never
  enters the gate, that its log echo never happens, that the slot stays spent,
  and that the kill-mode timeout message names the mode and the surviving
  invocations.

## Validation state

Linux: `make test-rust FILTER="--test server_async status_open_refresh"` (four
tests, including the reworked coalesced test and the new killed-child test) and
`make test-rust FILTER="--package magit-lsp-test-support kill_mode"` pass, and
`src/` is unchanged by this work.

Consumer-lane validation **pends a fresh release run**: the fix only makes a
host-killed Git child fail the test with its cause instead of a silent timeout,
so the macOS consumer job must be re-run to confirm either that the kill no
longer occurs or that its evidence now names the killed child. No cross-platform
claim is made from local Linux runs.
