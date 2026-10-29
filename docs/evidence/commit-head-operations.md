# Magit HEAD-edit operations: source evidence

Source checked 2026-10-29 against Magit
[`4031049049b23785268497ff49eed0690f1ce85f`](https://github.com/magit/magit/commit/4031049049b23785268497ff49eed0690f1ce85f),
`lisp/magit-commit.el`. Definitions `magit-commit-extend` (222–240),
`magit-commit-amend` (243–247), `magit-commit-reword` (250–268),
`magit-commit-assert` (450 onward), and `magit-commit-amend-assert` (441–448)
are the source basis. Git flag semantics are defined by
[`git-commit`](https://git-scm.com/docs/git-commit) (`--amend`, `--only`,
`--no-edit`, `--allow-empty`). Exact argv and date preservation are implemented;
focused real-Git acceptance is recorded by the Rust suites. Native Neovim
acceptance and focused VS Code native acceptance are recorded below.

* **Amend:** Magit invokes `git commit --amend` plus transient commit arguments
  with `magit-run-git-with-editor`. Git prepares an editor message; staged
  changes amend HEAD. An unchanged message remains valid and commits the
  existing message; an empty effective message is governed by Git/hooks.
* **Extend:** invokes `git commit --amend --no-edit` plus arguments. It amends
  staged changes without opening the editor or changing the message. When
  `override-date` is nil, Magit sets `GIT_COMMITTER_DATE` to the prior commit's
  committer date; the prefix/config option selects whether it updates.
* **Reword:** adds `--allow-empty` and invokes `git commit --amend --only` plus
  arguments through the editor. `--only` prevents staged changes from entering
  the amended tree; `--allow-empty` permits the same tree with a new message.
  Magit likewise preserves the committer date when `override-date` is nil.

**Date policy distinction:** the pinned source's two `*-override-date`
defcustoms actually default to `t` (update the date). This implementation's
requested contract preserves the prior committer date for Extend/Reword;
prefix/config date override is unavailable. This is not full default-config
Magit parity.

All three call the amend-published confirmation guard. `magit-commit-amend-assert`
uses `magit-list-publishing-branches` (in `lisp/magit-git.el`), testing ancestry
against configurable `magit-published-branches`. Our conservative guard asks
when HEAD is contained in any fetched remote-tracking ref; it does not expose
Magit's configurable publishing-ref policy or query a remote network.

Extend calls the shared argument assertion: it accepts staged changes or
unstaged changes with `--all`, or allow-empty/reset-author/signoff/author/date
metadata arguments. Without those it offers tracked `--all` for unstaged
changes, or errors “Nothing staged (or unstaged)”. Amend and Reword bypass
that assertion and permit clean inputs. Clean Extend requires a metadata
option such as `--allow-empty`; a byte-identical amendment with preserved
metadata can succeed without changing the HEAD OID. `--all`
stages modified/deleted tracked files for commit; untracked files are not
implicitly added. `--no-edit` is intrinsic to Extend and conflicts with the
purpose of Reword's editor; `--only`/`--all` semantics and options such as
message reuse/edit must be handled deliberately, not blindly concatenated.
Magit dispatches all three using the editor-capable runner, but Extend's
`--no-edit` means no message editor is expected. The real-file client workflow
must not substitute `git commit -F` or emulate Git's cleanup/hooks.

Focused implementation acceptance: assert exact argv and HEAD OID/message/tree,
index/worktree and committer dates for staged, unstaged (`-a`), and clean inputs;
verify editor opening versus no-editor Extend, unchanged-message amend, reword
tree preservation, cancellation/no mutation and readable conflicts. Include
native commit-menu selection and actual `COMMIT_EDITMSG` interaction in both
Neovim and VS Code. Do not expand to fixup, selection, rebase or reshelve.

## Verified implementation slice (2026-10-29)

* `tests/menu.rs`: both profiles select Amend/Extend/Reword from the menu,
  pop to the ancestor, and retain typed switches with exact action argv.
* `tests/commit_preflight.rs`: all three reject unborn HEAD and conflicting
  reuse/reedit or raw only/no-edit input without mutation; Reword
  rejects `--all`. Standalone reset-author is accepted for all three, including
  clean and unstaged-only Extend without staging or a tracked-all prompt.
  Reset-author with explicit author remains invalid; Create still requires
  reuse/reedit for reset-author. Published HEAD confirmation captures reset-author/no-verify argv;
  decline/dismiss, changed HEAD/index/worktree and closed origin start no worker
  and mutate no repository state. Extend's tracked-all offer is likewise
  captured/revalidated; clean Extend without metadata arguments fails readably.
* `tests/editor_bridge.rs::head_operations_real_git_tree_message_date_and_saved_close`:
  13 real-Git cases cover staged, unstaged, tracked-all, genuinely clean and
  cancellation inputs. Amend accepts the unchanged saved Git message; Reword
  edits the message without changing HEAD's tree or the staged index; Extend
  never opens an editor or changes the message. Exact HEAD OID/message/tree,
  index entries, worktree, commit count, untracked exclusion and committer dates
  are asserted, including a successful byte-identical Extend with allow-empty.
  Existing helper/run/version/cancellation, cleanup/hooks and security tests run
  unchanged through the same production pipeline; no `commit -F` path exists.

### Reset-author Rust follow-up (2026-10-29)

The same real-Git matrix adds nine `-R` cases (staged, clean, unstaged-only
for each action). Each starts with a nonempty HEAD, an actual parent, a distinct
original author and a fixed 1999 author date. The worker's supplied author identity
and 2026 author date replace both, preserving parents/count. Extend opens no
editor and retains the message; Reword retains HEAD's tree and the nonempty
staged index. Extend/Reword preserve the historical committer date. Existing
typed execution and published-HEAD capture are reused without lifecycle changes.

`cargo test --test commands --test commit_preflight --test editor_bridge --test menu --test menus && git diff --check`
passed (79 Rust tests). Focused native `-R` follow-up is recorded below;
the earlier native evidence alone does not establish reset-author success.
The feature parent checkboxes remain unchecked. Previously reported
phase failures in errors/log_commands/views and six full-UI scenarios are outside
this slice; this is not a full-suite success claim.

The exact ticket verification command passed (122 focused Rust tests,
`cargo test --no-run`, canonical manifest/menu/parity and VSCode keybinding
generators, `make vscode-test`, `git diff --check`). The menu snapshots actually
live at `tests/snapshots/menu-{60,140}.txt`; generators left these and the
manifest/keybinding snapshots unchanged. VSCode bundled declarations were
already current. Unrelated inherited release/license changes are not part of
this slice.

**Remaining at the implementation handoff (completed below):** VS Code native commit-menu/real
COMMIT_EDITMSG acceptance for all three actions. R-4.1-head-edit and the broader
R-EDIT-REAL parent remain unchecked. This is not a green full-suite claim:
the inherited `tests/errors.rs:477` failure remains recorded in TASKS.md.

## Native Neovim slice (2026-10-29)

`tests/ui/scenarios/commit-head-operations.scn` drives Amend, Reword, and
Extend from the rendered commit menu in both standard and vim profiles at
100x30 and 80x24. The exact verification command passed with exit 0:

```sh
cargo build --release && scripts/ui run tests/ui/scenarios/commit-head-operations.scn && git diff --check
```

The runner reported `1 PASS, 0 XFAIL, 0 FAIL` across all four combinations.
The scenario creates a nonempty baseline commit with a real parent, exact
subject/body and a fixed prior committer date. Fresh resets and checkpoints
isolate these cases:

* **Staged and clean Amend:** choose `a` from the real commit menu, save the
  unchanged Git-generated COMMIT_EDITMSG, then native `:q`. Exact message bytes,
  original parent list/count, index tree and worktree hash are asserted; HEAD
  changes. Staged Amend consumes the index tree. Only Extend/Reword promise
  preservation of the prior committer date.
* **Reword:** choose `w`, replace and save the real message with exact subject
  and body, assert save alone leaves HEAD unchanged, then close. The preflight
  explicitly proves the index is nonempty and differs from HEAD's tree. Exact
  edited message bytes, unchanged HEAD tree/index/worktree, original parents/
  count, changed OID and prior committer timestamp (including offset) are checked.
* **Extend:** choose `e` with staged content and assert exact retained message,
  prior committer date, index-derived HEAD tree, parents/count, changed OID and
  clean worktree. No editor input is sent; native screens remain status views
  and the active view is asserted not to be an editor document.
* **Captured `--all`:** toggle `- a` in the native menu, then Extend unstaged
  tracked changes alongside an untracked sentinel. Exact expected tree and
  worktree hashes prove tracked inclusion and untracked exclusion; message,
  committer date, parents/count and changed OID are checked too.
* **Clean Extend:** without options, readable refusal leaves exact HEAD/index/
  worktree unchanged. Toggle `- e` (`--allow-empty`) in a fresh menu and succeed
  without an editor. Exact identical HEAD OID proves all commit metadata and
  tree are retained; index/worktree remain unchanged.
* **Explicit cancel:** edit an Amend message with both staged and unstaged
  changes present, then invoke the client's explicit abort. Exact HEAD OID,
  count, index tree, worktree hash and porcelain state are unchanged. Native
  feedback currently reads `` `git commit` failed. ``; no success is inferred
  from that message.

Inspected all 15 current named snapshots in each of
`build/ui/commit-head-operations/{standard,vim}-{100x30,80x24}/`:
`amend-menu`, `amend-{staged,clean}-draft`,
`amend-{staged,clean}-unchanged`, `reword-{saved-draft,message}`,
`extend-staged-no-editor`, `extend-all-option`, `extend-tracked-all`,
`extend-clean-guard`, `extend-allow-empty-option`, `extend-clean-allowed`,
`amend-cancel-draft`, and `amend-explicit-cancel`. Draft screens show the real
COMMIT_EDITMSG filename and exact message/body. Option screens show `[x]` and
`git commit --all` / `git commit --allow-empty`; resulting status screens show
the expected staged/untracked counts. Earlier `failure.screen` files and old
`amend-unchanged` snapshots are stale failed-attempt artifacts, not this pass.

**Failure diagnosis:** the inherited scenario selected Create (`c`), not Amend
(`a`), and deleted the message, explaining its empty-message abort. The release
binary was rebuilt before testing. During completion, an incorrectly named
checkpoint briefly overwrote the disposable fixture's `.git/index`; it was
renamed `.git/head-index`. An empty baseline commit also made clean Amend fail
Git's empty-commit guard; a nonempty baseline fixes that test setup. No source
bug, production workaround, native quit interception, sleeps or deadline
increase was needed. An actual extra Neovim window keeps native `:q` from
exiting the editor, matching `commit-real-file.scn`.

The runner stopped all owned sessions; no owned server/Neovim process remained.
Inherited release/license work is untouched. This completes only the focused
Neovim slice (VS Code evidence follows): broader real-file acceptance and
remaining phase gates are still required. No full `make test` success is claimed.

## Native VS Code HEAD-only slice (2026-10-29)

The exact ticket command exited 0:

```sh
MAGIT_VSCODE_BIN=/home/william/.nix-profile/bin/codium MAGIT_VSCODE_HEAD_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check
```

Retained evidence: `/tmp/opencode/vscode-review-tAX9dE/`, especially
`head-operations-evidence.json` (pass, eight isolated cases, 26 named screenshots)
and `head-operations-trace.json` (requests, effects, draft notifications and
document-open observations). This is a normal production extension host, not an
extension-test host. Missing prerequisites fail the focused mode; the existing
180-second host bound and 15-second condition deadlines are unchanged.

The harness types `a`, `w`, `e`, `- a`, and `- e` into the real focused commit
QuickPick using native renderer input. Production menu routing supplies the
requests; exact server activation steps and captured option state are asserted.
No synthetic selections or direct HEAD-action invocation are used. Real
Git-generated COMMIT_EDITMSG files are saved, then their last native tabs closed
with the workbench. Each completion records exactly one saved-file confirmation;
save alone leaves HEAD unchanged. Extend opens no draft or draft-document
traffic. Explicit abort uses the production `magit.cancelEdit` command.

The nonempty baseline has a real parent, exact subject/body and committer date
`2001-01-01T00:00:00Z`. Every replacement retains the original parent list and
total commit count **2**, rather than creating a descendant:

| Case | Result HEAD | Exact state checked |
| --- | --- | --- |
| Staged Amend | `d954c60e702ea5465d2812654211d2b1ae48f2fb` | Unchanged saved message, index-derived tree, clean status |
| Clean Amend | `e338f885ac9f5584ebe7e8ff13f373032e37a4d0` | Unchanged saved message/tree/index/worktree |
| Edited Reword | `0d47d7ca4c363b42bcdfc75660b544a09e2662d5` | Exact edited subject/body, original HEAD tree, genuinely nonempty differing index retained, prior committer date |
| Staged Extend | `647fc66cda8d87a9350bb623fe32ea97fc9b8749` | No editor, exact retained message/date, index-derived tree |
| Tracked `--all` Extend | `4670b9533ace3b589db6e21c78bba1221af53dad` | Expected tracked tree/index entries, untracked sentinel hash preserved/excluded, only `?? untracked.txt` remains |
| Clean Extend guard | `637ac23c0d168912c8751416cb811804c1733c9d` | Readable refusal, exact state unchanged |
| Clean `--allow-empty` Extend | same baseline OID | Positive Committed effect, no editor, byte-identical HEAD/index/worktree |
| Explicit Amend abort | same baseline OID | Dirty edited draft, no confirmation, exact HEAD/index/worktree and `MM tracked.txt` retained |

JSON stores untrimmed Git `%B` output (including its formatting newline), tree
OIDs, full index entries, tracked/untracked content hashes, porcelain, parents,
count and `%cI` dates before/after each case. Reword's saved file is exactly
`Native reworded message\n\nExact edited body.\n`; unchanged operations retain
`Native HEAD baseline\n\nExact retained body.\n`. Amend does not promise date retention.

Inspected current screenshots `head-amend-clean-saved.png`,
`head-amend-staged-status.png`, `head-reword-{saved,status}.png`,
`head-extend-tracked-all-status.png`, `head-extend-clean-guard-status.png`,
`head-extend-clean-allowed-menu.png`, and `head-amend-abort-edited.png`.
They show real COMMIT_EDITMSG subject/body, retained Reword staging, tracked-all
untracked exclusion, the readable guard, allow-empty **on**, and dirty abort.
Native editor paint is polled against document lines before non-menu captures.
Earlier pass `/tmp/opencode/vscode-review-DBc9Fp/` additionally had inspected
staged Amend, editor-free Extend, both option menus and aborted staged/unstaged
status screenshots.

**Harness diagnosis:** inherited `Fwpubz` recorded just one queued Amend request
and zero completed drafts; `drafts.size === 0` before attachment was not
completion evidence. The older menu timeout and `key is not a function` trace
likewise proved no operation. Arrow-navigation probes left the QuickPick on its
initial row; the final scenario uses its supported typed-menu route. Large
screenshots exposed immediate subprocess exit truncating CDP JSON; awaiting
stdout drain fixes that harness transport. Reword legitimately holds Git's index
lock while editing, so save-alone checks read HEAD without write-tree during
the operation. Clean refusal is a response effect, not a notification; both are
now observed transparently. No production changes, added sleeps, deadline
inflation, lock deletion, synthetic didClose or fabricated dialogs were needed.

The suite deactivated its client and the runner terminated its owned host;
process inspection found no remaining magit-lsp or owned host. Only this focused
slice is complete: parent HEAD-edit and R-EDIT-REAL remain unchecked until phase
gates. Inherited release/license work is untouched. No full-suite claim is made.

## Native reset-author follow-up (2026-10-29)

The focused scenario now types `- R` then Reword in Neovim. A nonempty
historical commit has a real parent and author `Historical Author
<old@example.com>` dated `1999-02-03T04:05:06Z`. A separate staged sentinel
makes the index genuinely different from HEAD's tree. Saving the unchanged
real COMMIT_EDITMSG leaves HEAD unchanged; native `:q` completes it. Exact
message, tree, index tree and entries, worktree hash, porcelain, parents/count
and historical committer date remain unchanged; HEAD and author identity/date
change. Each field has an individual diagnostic, and the full before/after
state is retained as `reset-author-evidence.json` beside the native snapshots.
The expected current identity/date comes from `git var GIT_AUTHOR_IDENT` in
the fixture environment: **Test User**, `test@example.com`, **2000-01-01**.
This deterministic worker date is not the wall clock or the historical date.

The VSCode helper types the real QuickPick's `- R` activation steps for each
of Amend, clean Extend and staged Reword. Its historical author/date fixture
is the same; the current identity is **Test <test@example.com>** and current
author timestamps are bounded by the operation's start/end wall-clock seconds.
Amend updates the committer timestamp; Extend/Reword preserve 2001's timestamp.
All three retain exact message/parents/count, index entries and worktree;
Amend consumes the staged tree, clean Extend opens no editor or draft traffic,
and Reword retains the original HEAD tree and different staged index. Amend
and Reword save and close actual native file tabs, asserting save alone does
not commit and exactly one `saved-file` confirmation completes each operation.
The existing Rust matrix supplies all three actions' staged/clean/unstaged
coverage; this follow-up changes no production paths.

**Failed-attempt diagnosis:** the inherited reset-author Amend assertion
incorrectly expected `Test` (Neovim's fixture is `Test User`) and preserved
committer date (only Extend/Reword promise preservation). Earlier empty-commit
fixtures and sole-window native `:q` also prevented acceptance; the nonempty
fixture and real extra window are retained. The first diagnostic Reword run
reached successful `Committed: Reset author baseline` but correctly rejected
a wall-clock author-date expectation: the Neovim server inherits the harness's
fixed 2000 author date. Expectations now assert that exact configured current
date rather than merely checking inequality. No production workaround,
timeout increase, sleeps, ring or lifecycle changes were introduced.

A subsequent run passed all native state assertions but the VSCode launcher
rejected an evidence schema with eleven entries: it requires exactly the eight
original `cases`. The helper now retains those unchanged and records the three
additional cases in `resetAuthorCases` in both JSON files, asserting all three
completed before publishing `status: pass`. The launcher and its completion
gate were not weakened or modified.

### Final retained pass

The exact ticket command exited **0**:

```sh
cargo build --release && scripts/ui run tests/ui/scenarios/commit-head-operations.scn && MAGIT_VSCODE_BIN=/home/william/.nix-profile/bin/codium MAGIT_VSCODE_HEAD_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check
```

Neovim reported `1 PASS, 0 XFAIL, 0 FAIL` across standard/vim and 100x30/80x24.
Inspected all four variants' `reset-author-{option,saved,result}.screen` and
`reset-author-evidence.json` in `build/ui/commit-head-operations/`. Screens show
`--reset-author [x]`, the saved real COMMIT_EDITMSG, successful Committed feedback
and the retained staged sentinel. JSON records author timestamp **946684800**,
committer date **2001-01-01T00:00:00Z**, count **3**, and replacement
`cb4bb50bb0710e159fe171b007b0364706ed30ce` →
`ec298d2840631a77c1597ec3c57efe434346a62a` in every variant.

VSCode evidence is retained at `/tmp/opencode/vscode-review-nmM2OM/`:
`head-operations-evidence.json` has `status: pass`, eight original `cases`,
three `resetAuthorCases` and 39 named screenshots; the trace retains native
requests ending in `-, R, a, -, R, e, -, R, w` and saved-file confirmations.
Inspected current `head-reset-author-amend-saved.png`,
`head-reset-author-extend-menu.png`, and
`head-reset-author-reword-{saved,status}.png`: the real saved subject/body,
reset-author **on**, and retained Reword staging are visible. Old cancellation
failure toasts remain visible from the deliberately aborted earlier case;
they are not this operation's outcome. JSON proves current author timestamps
**1793251564**, **1793251567**, **1793251569**, each within its operation bounds;
all counts remain **3**. Result HEADs are respectively
`e423229c80226c5359bf0a1028f383c14bdafaeb`,
`a64e3f6d2e9a6bd82db8e6911453ec6f5cb3a10c`, and
`15d63d0dd6bf9ba1e9bf13db0eb9233d58245058`.

Process inspection after completion found no owned tmux/Neovim sessions,
release/debug magit-lsp servers or this isolated VSCode host. Rust acceptance
from `9002cbd` is reused; no unrelated suite or production change was needed.
The reset-author review is complete; HEAD-edit/real-file feature parents and
broad phase gates remain unchecked. Inherited release/license edits are preserved.
