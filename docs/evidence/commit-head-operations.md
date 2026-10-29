# Magit HEAD-edit operations: source evidence

Source checked 2026-10-29 against Magit
[`4031049049b23785268497ff49eed0690f1ce85f`](https://github.com/magit/magit/commit/4031049049b23785268497ff49eed0690f1ce85f),
`lisp/magit-commit.el`. Definitions `magit-commit-extend` (222–240),
`magit-commit-amend` (243–247), `magit-commit-reword` (250–268),
`magit-commit-assert` (450 onward), and `magit-commit-amend-assert` (441–448)
are the source basis. Git flag semantics are defined by
[`git-commit`](https://git-scm.com/docs/git-commit) (`--amend`, `--only`,
`--no-edit`, `--allow-empty`). Exact argv and date preservation are implemented;
focused real-Git acceptance is recorded by the Rust suites. Native commit-menu
acceptance in Neovim and VS Code remains pending.

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

**Immediately next:** native commit-menu/real COMMIT_EDITMSG acceptance for all
three actions in both Neovim and VS Code. R-4.1-head-edit and the broader
R-EDIT-REAL parent remain unchecked. This is not a green full-suite claim:
the inherited `tests/errors.rs:477` failure remains recorded in TASKS.md.
