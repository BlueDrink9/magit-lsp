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
at `build/startup-diagnosis/20261121T095024Z/`; `summary.json` contains version,
host, definitions and distributions. Each `*.json` contains sample status and
timings; each `*.trace.jsonl` is the wall-clock-merged client/server trace.
`*.events.jsonl` contains the raw display-check events. These are generated
artifacts and are not committed.

| Case | Fixture median (range), ms | Worktree median (range), ms |
|---|---:|---:|
| Fresh Neovim/server, command to visible `Head:` status | 734.7 (678.9–759.5) | 1082.4 (1052.0–1116.7) |
| Same initialized server, wipe and reopen status | 242.8 (225.5–274.0) | 593.4 (588.9–597.2) |
| Already-open view, focus only | 0.055 (0.046–0.056) | 0.063 (0.058–0.066) |

The “cold” label means a fresh Neovim and server process, **not** cold
filesystem/OS caches. Fixture and worktree are intentionally different
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
Thus the command-to-open-request interval is roughly 351 ms and the server
handler-to-render-application interval roughly 408 ms in this sample. Do not
sum these overlapping clock domains.

The same sample logs Git root discovery, status listing and cached-file diff
calls. Individual reported Git calls were typically 13–16 ms, several calls
occurred before/around status construction, and the traced handler's duration
was about 407 ms. This leaves substantial handler time not explained by those
Git spans (including uninstrumented session, view construction, rendering and
watcher work). It is not safe to label the complete handler as Git cost. In the
current modified worktree trace
(`worktree-1795254630996053483.trace.jsonl`), the first handler lasted about
719 ms and emitted many `ls-tree`/`diff` Git calls because the worktree itself
contained modifications. This is evidence that repository contents can expand
the Git workload, not evidence about the user's clean or configured checkout.

The measured `render_sent` to `render_applied` wall-clock gap is negligible in
these samples, but does not measure a real terminal/UI redraw or perceived
display. Buffer application and current-window `Head:` validation are the
client endpoint. No separate initialize reply, process-spawn span, LSP
descriptor callback, redraw notification, lock-wait, watcher-setup or individual
render-phase timing is available. Client/server wall clocks are merged using
wall time and may have offset/precision error; monotonic durations within each
process are stronger evidence. Tracing adds small overhead.

## Ranked diagnosis and next discriminating experiments

1. **Work before the first correlated open request is substantial.** The
   measured command-to-request gap is about 351 ms for the fixture, while the
   cold command-to-visible median is about 735 ms. Source: `init.lua:427-435`
   gates initial open on client initialization; `init.lua:134-154` sends the
   view request only after that path. Discriminate client startup, process spawn
   and initialize response by adding explicit lifecycle boundaries; do not
   infer which component dominates from this aggregate gap.
2. **Server open work is a major measured interval, with Git only partly
   attributed.** The fixture handler is about 407 ms; current modified worktree
   handler is about 719 ms with more Git calls. `view_execution.rs:24-92`
   places root discovery, session creation/lock, open/render and watcher setup
   in this path; `status_view.rs:40-51` reads status/rebase state
   synchronously. Add narrowly placed spans for lock wait, session/view/render,
   and watcher setup, then compare clean/dirty repositories of controlled
   sizes. The evidence does not yet justify a caching or lazy-load change.
3. **Git call multiplicity may explain some dirty-worktree growth.** The trace
   confirms multiple status and per-file Git operations. Quantify Git time and
   invocation counts against a clean commit and controlled file/hunk counts
   before considering batching. Current worktree data is confounded by this
   diagnostic's own uncommitted changes.
4. **Client render application is not currently the dominant measured gap.**
   Correlated fixture render delivery/application timestamps are nearly
   coincident with server send. Repeat through an attached interactive UI with
   a redraw event before concluding visible-pixel latency is negligible.

No optimization is prescribed from this run. The bounded next task is to add
diagnostic spans for initialization and server handler subphases and run a clean,
controlled fixture matrix; the reported user symptom needs reproduction with
their Neovim configuration and repository before a fix is selected.
