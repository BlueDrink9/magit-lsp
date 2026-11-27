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
post-spawn `setpgid`; Darwin uses `posix_spawn`'s `SETPGROUP` and inherits the
session — see Portability gate). No controlling tty: piped stdout/stderr, null stdin unless
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
Git's own terminal-prompt paths on Darwin stay closed by construction (see
Portability gate). Children that address `/dev/tty` directly can still reach an
inherited controlling terminal; unconditional KILL escalation remains their
containment.
Readers must support cancellation/parent-pipe close, bounded data and deadline,
not detached indefinitely-blocked threads. Crash cannot run this reaper or
guarantee child cleanup. Nested hooks share group; `setsid` escapees cannot be
contained. Never broad-kill, delete locks, or kill normal-exit daemons.

Portability gate: `setsid` and `waitid(...WNOWAIT)` are Unix interfaces with
platform-specific availability/semantics; Linux implementation and real-Git
tests are required before Linux R-EDIT-2 acceptance and R-EDIT-6 client cutover.
Linux is unchanged: session and group leader before exec, no controlling tty.

Darwin must not use `pre_exec` at all: any `pre_exec` forces Rust std's fork+exec
path, and its child-side libplatform atfork handlers abort mid-spawn
(`libSystem_atfork_child` → `_notify_fork_child` →
`_os_once_gate_corruption_abort`, `os_once_t is corrupt`; run 45971196440,
evidence in
[`docs/evidence/native-functional-status-gate-macos-kill.md`](../evidence/native-functional-status-gate-macos-kill.md)).
The Darwin guarantee is therefore **process-group ownership, not a new session**:
`CommandExt::process_group(0)` maps to `POSIX_SPAWN_SETPGROUP`, so pgid == pid
and the group is signalled via `-pid`, while the session is inherited and a
controlling tty remains possible. Git's own prompt paths are closed instead:
stdin is always null or piped and never inherited
(`src/runner/owned.rs:112-119`), `GIT_TERMINAL_PROMPT=0` is always set
(`src/git.rs:3450`), and `GIT_ASKPASS`/`SSH_ASKPASS` are routed through the
editor bridge whenever a server runs (`src/git.rs:3454-3459`). This does not close
a child that opens `/dev/tty` directly (for example ssh host-key confirmation);
when the server inherited a controlling terminal, that path remains reachable
and the escalation KILL is the containment backstop.

Spawn-path disqualifiers are audited. Rust std uses `posix_spawnp` for a bare
program name like `git` and bails to fork+exec only when: `pre_exec` sets
uid/gid/groups/chroot (`library/std/src/sys/process/unix/unix.rs:469-477`); an
env override of `PATH` accompanies a non-path program (:471); or a cwd is set
and the *runtime weak-symbol* probe for
`posix_spawn_file_actions_addchdir_np` (macOS ≥ 10.15) fails, or the program is
a relative path on Apple (:657-678). The posix_spawn entry point is
`posix_spawnp` (:783); these citations are from the upstream master source and
the checked local toolchain was rustc 1.99.1
([evidence](../evidence/native-functional-status-gate-macos-kill.md)).
Production Git commands now use no disqualifier: no `pre_exec`, no `PATH` env
key, the bare program name `git`, and the working directory passed as the git
global option `-C <cwd>` instead of `Command::current_dir` (both production
spawns, `git_command` and `user_excludes_path`, in `src/git.rs`). Hosted macOS
proof that these spawns actually take the posix_spawn path remains **pending
the next release run**; this is a structural audit, not host evidence.

PID reuse stays excluded on every Unix target: group signals are sent only before
the direct child is reaped, and the unreaped zombie pins pid/pgid for exactly as
long as ownership is retained. Grace escalation (TERM, 2-second grace, one group
KILL) is unchanged on Darwin. macOS group/WNOWAIT/reap behavior otherwise remains
unverified and belongs to the separate, later R-EDIT-9 CI/platform acceptance
ticket, not a Linux acceptance dependency. Retain the full-grace fallback on
macOS; it is not validation. Investigate kqueue
exit observation only if required, not as an implemented alternative. Rust
`Child::try_wait` reaps on observed exit, so cannot precede a possible group
signal; use non-reaping waitid observation plus sole final reap or gate platform.
Windows lacks this Unix group/waitid scheme; named-pipe ACL and suspended
creation/atomic Job assignment require independent R-EDIT-7 feasibility and
validation. Numeric limits are proposed defaults, not current behavior.
