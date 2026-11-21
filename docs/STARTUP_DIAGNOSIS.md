# Neovim `:Magit` opening diagnosis

This is a measured diagnostic, not an optimization report. It does not reproduce
the user's environment or establish a performance threshold. The user's
reported slowness remains unreproduced.

## Reproduction and evidence

Run `make nvim-startup-check`. The Make target builds the debug server and runs
the Python/Lua harness through the public `:Magit` command. It performs three
fresh-process samples each on a one-file temporary Git fixture and on the
current worktree. Each process also wipes/reopens the status buffer against its
initialized server and invokes `:Magit` once more while the view is already
open. The harness fails for missing event types, invalid status text/window,
open timeout, Neovim failure or missing binaries; it has no speed threshold.

Recorded environment: Neovim `NVIM v0.12.6`, debug server
`target/debug/magit-lsp`, Python `3.14.10`, Linux
`b9ff40574f5a` / kernel `7.2.11-0.fc44.x86_64`. The reproducible run is retained
at `build/startup-diagnosis/20261121T100628Z/`; `summary.json` contains version,
host, definitions and distributions. Each `*.json` contains sample status and
timings; each `*.trace.jsonl` is the wall-clock-merged client/server trace.
`*.events.jsonl` contains the raw display-check events. These are generated
artifacts and are not committed.

| Case | Fixture median (range), ms | Worktree median (range), ms |
|---|---:|---:|
| Fresh Neovim/server, command to visible `Head:` status | 570.8 (560.3–581.7) | 608.1 (586.5–711.7) |
| Same initialized server, wipe and reopen status | 210.0 (208.4–227.9) | 249.9 (243.5–284.8) |
| Already-open view, focus only | 0.056 (0.0–0.1) | 0.1 (0.0–0.1) |

The “cold” label means a fresh Neovim and server process, **not** cold
filesystem/OS caches. The harness additionally verifies the active window shows
the validated status buffer. Fixture and worktree are intentionally different
repositories; the worktree was dirty with diagnostic edits and generated build
artifacts during sampling. Its larger Git workload is not a clean baseline and
must not be generalized to the user's repository.

## Attribution and limits

The opt-in client trace allocates a correlation ID on initial `view/open` and
the existing server request tracing, Git-call tracing, and client render events
are merged with `t_unix_ms`. On the first fixture sample
(`fixture-1795254624569455965.trace.jsonl`), the first client open-trace event
was at elapsed 351.1 ms; the correlated server handler ran from 0.17 to 407.2
ms on its own clock (about 407 ms), with `render_sent` at 407.3 ms. Client
`render_received` and `render_applied` were at 759.4 and 759.6 ms on the
client clock; their merged wall times were within about 1 ms of `render_sent`.
The earlier ~351 ms request gap and ~407 ms handler were a single sample, not
components additive to the separate ~735 ms median. The new run's fixture cold
samples are 560–582 ms; correlated handler spans are 311–321 ms and traced Git
calls total 144–151 ms (11 calls/sample). Worktree cold samples are 587–712 ms;
handlers are 334–436 ms and Git totals 164–244 ms (12 calls/sample). These are
per-sample measurements; do not add medians or overlapping clock domains. The
elapsed start and stop are immediately around public `:Magit` and validated
status display in the active window.

The same sample logs Git root discovery, status listing and diff calls. Handler
time beyond traced Git is not classified and may include session, view, render
and watcher work; this does not prove a single source-level cause. The
worktree's own modifications make it nonrepresentative of a clean user checkout.

### Repository-discovery reuse comparison (2026-11-21)

For PERF-OPEN-REUSE, the same committed harness and debug profile were run
serially from an archive of pre-change `1863cfd` and the post-change worktree;
both used NVIM v0.12.6, Python 3.14.10, Linux `b9ff40574f5a`, three samples,
and the harness's one-file temporary fixture. Raw baseline data and merged
traces are in `build/startup-before-auNEBO/build/startup-diagnosis/20261121T103133Z/`;
post-change artifacts are in `build/startup-diagnosis/20261121T103146Z/`.
The measured fixture cold medians were 744.3 ms before (606.3–755.1) and
506.3 ms after (499.2–538.8); initialized-server reopen was 229.0 ms
(227.3–266.5) and 176.6 ms (166.3–180.0), respectively. Already-open focus
medians were 0.055 and 0.063 ms. Across each three cold fixture traces, Git
calls fell from 22 to 16 per sample and `rev-parse` calls from 14 to 8; the
traces therefore show the repeated discovery reduction, not its isolated
contribution to wall time.

Observed timings improved in these samples, but this is not a controlled
speedup claim: there are only three samples per side, host load and filesystem
caches vary, and process-cold does not mean cache-cold. The larger cold and
reopen reductions are consistent with the removed discovery work but cannot
attribute the full difference to it. Existing measurements still rank the
server handler as the largest measured internal span, with substantial
unclassified non-Git handler time; finer session/view/render/watcher spans are
the next discriminating measurement, not another implementation.

The measured `render_sent` to `render_applied` wall-clock gap is negligible in
these samples, but does not measure a real terminal/UI redraw or perceived
display. Buffer application and current-window `Head:` validation are the
client endpoint. Client/server wall clocks are merged using wall time and may
have offset/precision error; monotonic durations within each process are
stronger evidence. Tracing adds overhead. This is instrumented wall-time
profiling, not CPU profiling; external `perf`/`strace` evidence is still needed
to attribute CPU, scheduler and syscall costs.

## Ranked diagnosis and next discriminating experiments

### Granular instrumented run (2026-11-21)

`make nvim-startup-check` now retains correlated per-open phase durations and
ranked Git calls in each sample's `profiles_by_trace_id`. Full output is in
`build/startup-diagnosis/20261121T104046Z/summary.json` and its per-sample
`*.json`/`*.trace.jsonl` files. It ran three fixture and three worktree
processes; each has a cold open and an initialized-server reopen. Wall-time
medians (ranges) were fixture cold 522.3 ms (479.3–551.8), worktree cold
950.3 ms (929.7–1087.2), fixture reopen 174.0 ms (169.7–174.7), and worktree
reopen 619.4 ms (618.4–626.9). Already-open focus was 0.044 ms fixture and
0.101 ms worktree median.

For the fixture, median traced Git duration was 106 ms on both cold and reopen;
the cold first sample's longest commands were `rev-parse --show-toplevel` and
cached diff at 14 ms each, followed by several 13 ms commands (ties, not a
single dominant command). For the dirty worktree, median traced Git duration
was 481 ms cold and 486 ms reopen. Its per-sample leader was repeated
`git diff --name-status -z --find-renames` (up to 18 ms); status and per-file
diff/index queries also appeared. Worktree results reflect this dirty
diagnostic tree and must not be generalized to a clean user repository.

Median handler-minus-summed-Git remainder was 147.5 ms fixture cold, 61.4 ms
fixture reopen, 204.4 ms worktree cold and 122.7 ms worktree reopen. This is a
measured non-Git remainder, **not** exclusive CPU time or proof that one phase
owns the gap. Inclusive fixture-cold phase medians were 41.0 ms discovery,
92.9 ms status listing, 28.0 ms hunk acquisition, 28.2 ms tree render and
120.9 ms combined session open/render; these overlap and must not be added.
Rebase and watcher setup were negligible in fixture samples. In the worktree,
median inclusive status listing was 324.4 ms and tree rendering 195.6 ms; these
identify status/tree construction as the next area to investigate, but their
overlap and Git work inside those spans prevent exclusive attribution. Hunk
acquisition was around 32 ms.

The server emits repository-lock wait only on the queued mutation path; measured
`view/open` does not go through it, so this does not measure contention on the
session `Mutex`. Server initialization has a phase event but is not correlated
to the `view/open` trace; client LSP setup/initialize callback, process
spawn-to-request, and visible redraw remain unavailable boundaries, not
zero-cost phases. Inclusive spans overlap; handler-minus-Git is only the
arithmetic remainder. Three samples per repository show variance (notably
worktree cold 929.7–1087.2 ms), so medians are descriptive, not controlled
performance claims. Profiling overhead is present and not independently
calibrated.

**Next experiment:** use the user's planned `perf`/`strace` on the same public
command with cold/reopen cases, paired with clean and dirty worktree runs. Test
whether status-listing subprocess/filesystem activity explains the larger
worktree span; obtain syscall/CPU attribution before proposing batching or
another code change. No optimization follows from wall-time spans alone.

### Syscall and subprocess tracing (2026-11-21)

`make nvim-startup-strace` exits 0 (`ok: nvim-startup-strace`). It builds the
debug server, supplies it to the same public-command Python/Lua harness through
a generated executable `$MAGIT_LSP_BIN` wrapper, and runs strace 7.3 with
`%process,%file`, following forks. Each fresh server launch gets its own trace
file; the run's `summary.json` and raw traces are in the ignored
`build/startup-strace/20261121T231900-8241/`. Six launches were measured (three
one-file fixture, three this dirty worktree), and every trace contained the
server executable, child Git execs, filesystem calls and wait calls. The target
fails on an absent/unusable tracer, fewer, duplicate, empty or missing traces,
or a trace without a server exit record.

Observed per launch:

| Launches | Git execs | Filesystem syscalls | Traced syscall time, ms |
|---|---:|---:|---:|
| Fixture ×3 | 27 each | 3,322 each | 61.7 / 81.6 / 143.8 |
| Worktree ×3 | 177 / 177 / 231 | 70,890 / 70,898 / 86,250 | 1,999.7 / 2,256.4 / 2,033.3 |

Ranked median per-launch syscall time across all six launches: `newfstatat`
371.0 ms (17,695 calls), `openat` 209.2 ms (6,972), `statx` 138.2 ms (9,320),
`execve` 95.2 ms (103), `clone` 72.3 ms (101), `access` 38.5 ms (1,481),
`inotify_add_watch` 16.5 ms (998), `waitid` 15.7 ms (604), `getcwd` 6.9 ms
(339) and `chdir` 4.9 ms (135). These are syscall durations summed per launch,
not exclusive CPU or wall time: `waitid`/`wait4` block on child Git processes
whose own filesystem and process syscalls run concurrently, so categories
overlap and must not be added. Median blocking `wait4` time was 4.3 ms over 104
calls; the observed cost sits in the volume of filesystem/probe syscalls and
process creation around child Git, not in `wait4` blocking.

Limits: strace adds ptrace and record overhead that is not calibrated against
the harness, so traced totals (up to ~2.3 s per worktree launch) are not
comparable with its wall-time spans; the server is a debug build. strace
reports syscall/service and blocking time but no CPU time, so no CPU or hotspot
claim follows. Three samples per repository, a dirty worktree and a shared host
make the values descriptive, not controlled. The endpoint is the harness's
visible `Head:` status in a valid window, not terminal redraw or first-window
creation. Raw traces keep local paths and arguments and remain in ignored
`build/`; only aggregates are recorded here.

## Early loading display evidence (2026-11-21)

`make ui-file FILE=tests/ui/scenarios/status-loading.scn` exits 0
(`ok: ui-file`). The permanent scenario uses the public `:Magit` command,
`fixture: modified-two-files` and `startup: held`, so the first Git `status`
subprocess is held while the client is already running. Text snapshots retain
the lifecycle (`build/ui/status-loading/`; per-combination copies under
`standard-80x24/`):

- `loading.screen`: the active status window already shows the server text
  `Loading repository status…`, with no `Head:` or `Unstaged changes` content.
- `responsive.screen`: while the gate is still entered and unreleased, the
  registered `common.toggle-text-mode` key (`ctrl+t`, sent as `C-t`) makes the
  buffer user-owned; the scenario types `hello` and observes it on screen. A
  second `C-t` returns to the server-owned loading document, proving the input
  was processed rather than merely sent.
- `fresh.screen`: `release status` then the same view shows `Head: main`,
  `Unstaged changes (2)`, `M f.txt`, `M g.txt`, the hunk header
  `@@ -1,10 +1,10 @@` and `+line3 CHANGED`; the loading text is absent. The
  scenario waits for the rendered heading instead of treating client idle as
  readiness, because the outstanding Git call is server-side work.

This is display and lifecycle evidence from real Neovim, not a timing
measurement: it does not show a CPU speedup, a terminal/UI redraw latency, or
any threshold improvement. Snapshots are text (`.screen`/`.ansi`), no images.
The harness derives the run directory and RPC socket from the checkout root
(`scripts/ui_lib/paths.py:5-13`); in this worktree that path is 121 bytes,
above Neovim's 107-byte socket limit, so the verification ran through a
bind mount of the same checkout at a shorter path. That is a harness
path limitation, not a product behavior finding. No `magit-lsp` process
remained after the run.
