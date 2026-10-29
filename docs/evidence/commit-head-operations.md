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
acceptance is recorded below; VS Code native acceptance remains pending.

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
  reuse/reedit/reset-author or raw only/no-edit input without mutation; Reword
  rejects `--all`. Published HEAD confirmation captures author/no-verify argv;
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

The exact ticket verification command passed (122 focused Rust tests,
`cargo test --no-run`, canonical manifest/menu/parity and VSCode keybinding
generators, `make vscode-test`, `git diff --check`). The menu snapshots actually
live at `tests/snapshots/menu-{60,140}.txt`; generators left these and the
manifest/keybinding snapshots unchanged. VSCode bundled declarations were
already current. Unrelated inherited release/license changes are not part of
this slice.

**Remaining after the Neovim slice below:** VS Code native commit-menu/real
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
Neovim slice: VS Code native evidence, broader real-file acceptance and
remaining phase gates are still required. No full `make test` success is claimed.
