# Process ownership

**Process feasibility gate (proposed R-EDIT-2 mechanism).** One async owner task
owns `Child`, cancel receiver, exit observation, final wait/reap, and reader
completion channels; handlers only enqueue idempotent cancellation. A valid
progress-cancel request promptly receives one LSP `report` (`Cancelling…`, no
longer cancellable) and readable feedback. This acknowledges intent, not Git
termination or absence of a commit. Serialize report/end against token removal;
duplicate/unknown/ended tokens do not report, and cleanup retains the repository
reservation until the owner finishes. Exit-before-cancel still wins and completed
commits still report success. Start Git in
a new session/process group before exec (child-side spawn setup, not racy
post-spawn `setpgid`). No controlling tty: piped stdout/stderr, null stdin unless
the invocation requires otherwise. Concurrent readers retain at most 4 MiB per
stream, continue draining/discarding after cap, and flag truncation. Reader drain
deadline is 2 seconds after leader exit/terminal cancellation. When the owner
dequeues cancel, it first observes leader status with
`waitid(P_PID, pid, WEXITED|WNOHANG|WNOWAIT)`. If already exited, exit wins and
no signal is sent. Otherwise cancellation linearizes: abort a connected helper,
send TERM to the group, and send CONT to the group so stopped members can
process termination. Retain the unreaped leader for at most the 2-second TERM
grace, then send exactly one group KILL even if the leader exited during that
grace. Linux may finish grace early **only after WNOWAIT observes leader exit**:
inspect `/proc` for live nonleader members of its pinned process group, validating
stat identity/state/pgrp outside the potentially arbitrary `comm` field. No live
members observed is merely a latency hint: still send the final group KILL before
reap. A fork/scan race can miss a member and end its grace early; inspection is
neither a graceful-exit guarantee nor whole-descendant containment. Never signal
individual scan PIDs. Unavailable, malformed, slow or uncertain inspection keeps
full grace, as do observed live/stopped/TERM-resistant members. Other Unix targets
retain the existing full-grace fallback, without platform validation claims.
Reap only after KILL, using bounded observation; if KILL is not reaped within a further 2 seconds,
transfer the still-owned Child handle to a process-lifetime reaper task that only
waits/reaps; report cancellation incomplete and retain ownership until reap.
The retained unreaped leader pins PID/PGID identity: no other waiter, `try_wait`,
`waitpid(-1)`, or SIGCHLD auto-reap disposition is permitted. Never signal after
final reap. Exit observation is distinct from pipe EOF. On normal leader exit,
never signal to force inherited-pipe EOF; allow the 2-second drain deadline,
report incomplete drain, finalize.
Readers must support cancellation/parent-pipe close, bounded data and deadline,
not detached indefinitely-blocked threads. Crash cannot run this reaper or
guarantee child cleanup. Nested hooks share group; `setsid` escapees cannot be
contained. Never broad-kill, delete locks, or kill normal-exit daemons.

Portability gate: `setsid` and `waitid(...WNOWAIT)` are Unix interfaces with
platform-specific availability/semantics; Linux implementation and real-Git
tests are required before Linux R-EDIT-2 acceptance and R-EDIT-6 client cutover.
macOS group/WNOWAIT/reap behavior remains unverified and belongs to the separate,
later R-EDIT-9 CI/platform acceptance ticket, not a Linux acceptance dependency.
Retain the full-grace fallback on macOS; it is not validation. Investigate kqueue
exit observation only if required, not as an implemented alternative. Rust
`Child::try_wait` reaps on observed exit, so cannot precede a possible group
signal; use non-reaping waitid observation plus sole final reap or gate platform.
Windows lacks this Unix group/waitid scheme; named-pipe ACL and suspended
creation/atomic Job assignment require independent R-EDIT-7 feasibility and
validation. Numeric limits are proposed defaults, not current behavior.
