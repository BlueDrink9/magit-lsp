## 6. View catalog

The final column references stable parent task IDs in §7; completed status/process
work is summarized in the archive, managed editor work is R-EDIT, and todo is R7.3.

Status common hierarchy navigation and four-level visibility cycling are
implemented and verified by R-NAV (§7); these are server-tree operations.

| View | Magit analog | Contents | Key commands | §7 parent task |
| --- | --- | --- | --- | --- |
| **Status** | `magit-status` | Header lines (Head, Merge, Push, Tag); sections: in-progress operation, Untracked, Unstaged, Staged, Unmerged, Stashes, Unpulled, Unpushed, Recent commits, Submodules | stage/unstage/discard/apply/reverse, visit, toggle, refresh, all menus | Completed (§10/archive); R-3.6; R-6 |
| **Log** | `magit-log` | Commit list with graph, decorations, author, date; options (`-n`, `--graph`, `--decorate`, `--author`, `--since`, `--grep`, `-S`, `-G`, path limits); lazy "load more"; **select mode** | show commit, checkout/reset/branch/rebase/cherry-pick/revert from the commit at point, confirm/cancel | R-5.1 (completed; archive) |
| **Log variants** | `l` menu | current, other, all, local branches, reflog, file log, line-range log (`-L`), `HEAD..upstream`, `upstream..HEAD` | same | R-5.3 |
| **Revision** | `magit-revision` | Header (hash, parents, author, committer, dates, refs, message), diffstat, full diff with hunks; parent/child navigation; combined diff for merges | visit file, apply/reverse hunk, copy hash | R-5.2 |
| **Diff** | `magit-diff` | Any range `a..b`, `a...b`, worktree vs index vs revision, path-limited, stash vs parent, stat-only; context/whitespace/rename/word-diff options | visit, stage/unstage when applicable | R-5.2 |
| **Refs** | `magit-show-refs` | Local branches, remotes, tags with ahead/behind vs a base; also select mode | checkout, create, delete, rename, reset, merge, rebase onto, log | R-5.3 |
| **Cherry** | `magit-cherry` | `git cherry` between two refs | cherry-pick, show | R-7.2 |
| **Stash list / show** | `magit-stash-list` | Stashes; show = diff view of a stash | apply, pop, drop, branch | R-6.5 |
| **Rebase todo** | `git-rebase-mode` | Structured paused or prepared initial todo with pick/reword/edit/squash/fixup/drop and full-OID commit rows | `p`/`r`/`e`/`s`/`f`/`d` change action; `K`/`J` move row; `ctrl+s` submits; `Escape` cancels | R-TODO / R-7.3 |
| **Rebase in progress** | status section | Todo position, stopped commit, conflicts | continue, skip, abort, edit todo | R-OP-STATE / R-7.3 |

The paused merge-backend todo editor rejects `update-ref` commands; repositories using `rebase.updateRefs=true` cannot use it. A prepared initial todo is removed after execution starts, including when `edit` pauses Git or replay conflicts; the caller remains usable for refresh, conflict resolution and explicit `rebase.continue` or abort. A started execution failure reports its readable Git effect without restoring the prepared editor or automatically aborting the real operation.

Prepared initial todos may display `reword` and `squash`, but launch rejects those actions: prepared message editing is not supported yet.

Instant fixup/squash preparation supports only a non-root target in the linear
ancestry of `HEAD`, with staged changes and no tracked unstaged changes,
untracked files, or active Git operation. Preparation is read-only and binds
the repository, full target-to-`HEAD` membership, `HEAD` attachment, and exact index.
| **Merge / conflicts** | `magit-merge` + unmerged | Unmerged section; per-conflict actions | take ours/theirs, mark resolved, open merge tool (client effect `openMergeTool`), continue, abort | R-7.1 |
| **Sequencer** | `A`, `V` | In-progress cherry-pick/revert | continue, skip, abort | 7 |
| **Commit message** | with-editor buffer | Native real Git file; companion staged diff, history recall (R-EDIT-HISTORY), trailers and lint remain roadmap | confirm, cancel; planned history prev/next, add trailer | R-EDIT |
| **Process log** | `magit-process-buffer` | Every Git invocation: args, exit code, duration, output, hook output | visit, copy command, clear | Completed (archive) |
| **Blame** | `magit-blame` | Annotated file (hash, author, date, summary); also inlay hints/code lens in real file buffers | show commit, blame at parent, copy hash | R-8.1 |
| **File at revision** | `magit-find-file` | Virtual doc `magit://blob/<rev>/<path>` | diff against worktree | R-5.3 |
| **Bisect** | `magit-bisect` | In-progress section: good/bad/skip log, candidate | good, bad, skip, run, reset | R-8.2 |
| **Worktrees** | `magit-worktree` | List with branch/path/state | add, checkout, move, delete, visit | R-8.2f |
| **Submodules** | `magit-submodule` | Modules section and list | add, register, update, sync, fetch, populate, unpopulate | R-8.2 |
| **Remotes / config** | `magit-remote`, `magit-gitconfig` | Remote list; per-branch/remote config | add, rename, remove, prune, set upstream/pushRemote | R-6.3 |
| **Patches** | `w`, `W` | `git am`/`apply`, `format-patch`; in-progress am | apply, continue, skip, abort | R-8.2 |
| **Notes / ignore / run** | `T`, `i`, `!` | Notes; gitignore targets; run arbitrary git command | per command | R-8.2a |
| **Repository list** | `magit-list-repositories` | Multi-repo overview | open status | R-9.3 |
| **Dispatch / help** | `magit-dispatch` | Generated from the command registry and keymap manifest | invoke by key | Completed (§10/archive) |
| **Menus** | transients | commit, branch, fetch, pull, push, merge, rebase, stash, tag, reset, cherry-pick, revert, bisect, remote, submodule, worktree, diff, log, ignore, patch | §3.11 | R-5.1 / R-5.2 |

---
