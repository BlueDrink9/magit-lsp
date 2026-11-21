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
client endpoint. No separate initialize reply, process-spawn span, LSP
descriptor callback, redraw notification, lock-wait, watcher-setup or individual
render-phase timing is available. Client/server wall clocks are merged using
wall time and may have offset/precision error; monotonic durations within each
process are stronger evidence. Tracing adds small overhead.

## Ranked diagnosis and next discriminating experiments

1. **Server handler is the largest measured internal span.** Fixture handlers
   take 311–321 ms and worktree handlers 334–436 ms; traced Git accounts for
   144–151 ms and 164–244 ms. Remaining handler time is unclassified.
   `view_execution.rs:58-92` bounds session/open/render/watcher;
   `status_view.rs:40-51` invokes status/rebase reads. Next experiment: add
   monotonic spans around session lock, view construction/render and watcher
   setup, without changing behavior.
2. **Git work is substantial, not a proven sole bottleneck.** There are 11
   fixture and 12 worktree calls; summed Git durations leave significant handler
   time. `status_view.rs:40-51` establishes status/rebase operations. Compare
   identical clean/dirty fixture states and rank command names by duration
   before considering batching.
3. **Command-to-first-request delay remains unclassified.** Initialization
   gates open (`init.lua:427-435`, `init.lua:134-154`), but spawn, initialize
   reply and descriptor callback boundaries are not measured. Add those events
   next; the cold/reopen totals do not establish a particular cause.
4. **Client render application is not currently the dominant measured gap.**
   Correlated fixture render delivery/application timestamps are nearly
   coincident with server send. Repeat through an attached interactive UI with
   a redraw event before concluding visible-pixel latency is negligible.

No optimization is prescribed from this run. The reported user symptom needs reproduction with
their Neovim configuration and repository before a fix is selected.
