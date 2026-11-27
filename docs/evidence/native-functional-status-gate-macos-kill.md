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

## Reading the kill evidence

`GatedGit::install` now writes `spawned pid=$$ ppid=$PPID t=<epoch>` to the
invocation log as the gated script's first write, in both `arm` and
`arm_killing` modes. Identity is written before any fork (the epoch follows in a
second write), so even a child killed during exec setup leaves its pid, its
parent pid and (when it lived past the identity write) a timestamp. The parent
also logs every term, grace kill, cleanup kill, still-running child handed to
the background reaper and signal-death reap at `warn`, so those lines pass the
default `warn` filter into the server's stderr, which `wait_for_gate_or_failure`
returns with an acquisition failure and includes in its timeout panic.

Read the two sources together, in this order:

1. **Spawned line + a parent kill/term line for the same pid** — self-inflicted:
   the server's own cancellation or cleanup killed the child. This is a
   production bug and gets a follow-up ticket; the trace names the reason and
   the signal.
2. **Spawned line + a signal-death reap (`child reap ... signal=N`) with no
   parent kill line** — external: something outside the server killed the child.
   Re-query host logs for the test-step timestamp (macOS `log show --start
   <timestamp>`), the sender (kernel or another process), and the pid from the
   spawned line.
3. **No spawned line** — exec-time kill: the script never ran, so the killer
   acted between spawn and exec (code signing, jetsam). The reap trace still
   names the signal and pid.

An ordinary successful reap stays at `debug`, so this adds no per-invocation
warning noise.

## Verdict (run 45903898257, 2026-11-26): EXTERNAL kill

The discriminating evidence finally arrived and resolves the matrix to branch
2. In the macOS `native-functional-consumers` lane the failing
`status_open_refreshes_while_loading_are_coalesced` panicked with
`AcquisitionFailed` whose embedded server stderr showed:

- `child spawn pid=20793 reason="spawn" program=git
  args=["--no-optional-locks", "status", "--porcelain=v2", "-z", "--branch"]`
  at 22:35:40.265;
- `repository watcher armed` at .273 and one watcher raw event `kind=Other`;
- `child reap pid=20793 reason="reap" signal=9` at .280 — **18 ms after
  spawn**, with **no `child term` or `child kill` line** for that pid.

Branch 1 (self-inflicted) is excluded by construction: every server-side kill
site logs before signalling (`src/runner/owned.rs:216/221`, `:227/230`,
`:410/413`, `:444/446`; `src/git.rs:3319/3325`, `:3345/3351`), there is no
`Drop`-kills-child implementation and no tokio `kill_on_drop`, and the gated
status acquisition runs with **no cancel handle** (`src/server/view_execution.rs:216-220`),
so no supersede/refresh path can reach a kill. A watcher event only bumps the
status generation; it never signals the child (`src/server.rs:648-699`).

The stub's self-kill is excluded: the test uses `GatedGit::arm("status")`
(`tests/server_async.rs:1719`), not `arm_killing`; the fixture `Drop` kill fires
only after a release plus a 5 s exit timeout with an entered gate — neither held.

Branch 3 is excluded: the child spawned successfully and ran (the parent's
spawn warn precedes an 18 ms lifetime).

So an external actor SIGKILLed the fresh git-stub child ~18 ms after spawn. The
`os-kill-forensics` artifact for the run contains **no kernel or userland kill
record for pid 20793** — only launchd tearing down its own login-window
services (a GUI-session churn at 22:26, unrelated). Five occurrences across
lanes (functional ×3, build ×2) fit a macOS-26 hosted-runner environmental
factor that kills short-lived helper processes without a logged trail.

Standing disposition: record-only (this file), keep the fail-fast
`AcquisitionFailed` behaviour, no retry — a retry cannot fix an external
killer and would hide its recurrence. Validation of a lane that lost a test to
this kill is by re-running the job until green.

## Verdict (run 45971196440, 2026-11-27): fork pre-exec child-side abort

The next release run's `.ips` captures resolve the earlier "external killer with
no logged trail": the child was never killed from outside — it aborted on the
**child side of fork, before exec**. Two identical incidents were collected in
that run, at 09:26:51 and 09:27:24 (for example
`diagnostic-reports/user-magit-lsp-2026-11-27-092724.ips` under
`profile-one/`).

- Victim pid 18885: `child spawn pid=18885 reason="spawn" program=git`
  at 09:27:24.482040, then `child reap pid=18885 reason="reap" signal=9`
  **13 ms** later, still with no `child term` or `child kill` line — so branch 1
  (self-inflicted) remains excluded exactly as in the previous verdict, and the
  profiler's parsed reap victim pid matches the `.ips` report's victim.
- The `.ips` backtrace is child-side and pre-exec: `libSystem_atfork_child` →
  `_notify_fork_child` → `_os_once_gate_corruption_abort`, reported as
  `BUG IN CLIENT OF LIBPLATFORM: os_once_t is corrupt` and
  `crashed on child side of fork pre-exec`.
- The earlier absence of any ES/kernel kill record is explained by this: no
  actor signalled the child; the abort happens in the fork child, and the parent
  only observes a SIGKILL-class reap. A foreign atfork initializer stays
  unproven and is not chased (watcher-first-use is SUSPECTED only).

The fix is therefore a **spawn-path change, not a kill disposition**:
`src/runner/owned.rs` installs `pre_exec(setsid)` only on non-Apple Unix, because
any `pre_exec` forces std's fork+exec path and reproduces the abort. On Darwin the
child's process group is set with `CommandExt::process_group(0)`
(`POSIX_SPAWN_SETPGROUP`), which keeps group ownership (`kill(-pid)`) with the
session inherited; the verified terminal-access invariants above
(stdin never inherited, `GIT_TERMINAL_PROMPT=0`, askpass bridge) close the
resulting tty risk. `process_group` has been stable since Rust 1.64; the checked
local toolchain was rustc 1.99.1. The Darwin pgid == pid test cannot run on a
Linux worker: hosted macOS stays **unverified until the next release run**, which
is the real oracle.

## Host verdict capture (pending hosted run)

`scripts/profile_ci_test.py` now tees the wrapped command's stdout/stderr into
`child-output.log` with drain threads that forward every chunk to the caller's
inherited descriptors as it arrives, so the caller's bytes and timing are
unchanged. After the child exits, and only on macOS, it runs two best-effort
collectors whose statuses are recorded in `summary.json` under their own keys:

- `ips_reports` copies the code-signing/jetsam `.ips` reports written during the
  command window from `/Library/Logs/DiagnosticReports` and
  `~/Library/Logs/DiagnosticReports` into `diagnostic-reports/` inside
  `--output-dir`. A missing directory records `absent`, an unreadable one
  `denied`, a non-Darwin host `unavailable`, and a copy failure `denied`.
- `kernel_log` runs `log show --info --debug --start <start> --end <end>
  --predicate 'sender == "kernel" OR eventMessage CONTAINS[c] "<pid>"'` for the
  pids parsed from the server's `child reap pid=N ... signal=9` lines, and
  writes `kernel-log.log`. With no parsed victim pid it falls back to the
  kernel-only predicate. A missing `log`, a timeout or a non-zero exit is
  recorded, never fatal.

Neither collector can change the exit status or the caller's streams; both
behavioural properties are proven by `make profiler-check`, and the script
tokens are guarded by `make release-automation-unit`. Hosted evidence remains
**pending the next release run**: the existing `macos-profile-*` upload takes
the whole `$RUNNER_TEMP/macos-profile` directory, so the next macOS job carries
`child-output.log`, `kernel-log.log` and any `diagnostic-reports/` copies with
no workflow change. No hosted `.ips` or kernel-log capture is claimed here.

Next step: sender-capture and memory profiling now wraps both macOS lanes
(`scripts/profile_ci_test.py`, uploaded as `macos-profile-*`), and the same
lanes collect the host verdicts above; hosted capture remains **pending the
next release run** and is not claimed here.
