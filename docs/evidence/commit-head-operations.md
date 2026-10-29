# Magit HEAD-edit operations: source evidence

Source checked 2026-10-29 against Magit
[`4031049049b23785268497ff49eed0690f1ce85f`](https://github.com/magit/magit/commit/4031049049b23785268497ff49eed0690f1ce85f),
`lisp/magit-commit.el`. Definitions `magit-commit-extend` (222–240),
`magit-commit-amend` (243–247), `magit-commit-reword` (250–268),
`magit-commit-assert` (450 onward), and `magit-commit-amend-assert` (441–448)
are the source basis. Git flag semantics are defined by
[`git-commit`](https://git-scm.com/docs/git-commit) (`--amend`, `--only`,
`--no-edit`, `--allow-empty`). This is source evidence, not an implementation
claim.

* **Amend:** Magit invokes `git commit --amend` plus transient commit arguments
  with `magit-run-git-with-editor`. Git prepares an editor message; staged
  changes amend HEAD. An unchanged message remains valid and commits the
  existing message; an empty effective message is governed by Git/hooks.
* **Extend:** invokes `git commit --amend --no-edit` plus arguments. It amends
  staged changes without opening the editor or changing the message. By default
  Magit sets `GIT_COMMITTER_DATE` to the prior commit's committer date; its
  prefix/config option can instead allow the date to update.
* **Reword:** adds `--allow-empty` and invokes `git commit --amend --only` plus
  arguments through the editor. `--only` prevents staged changes from entering
  the amended tree; `--allow-empty` permits the same tree with a new message.
  Magit likewise preserves the committer date by default, with a prefix/config
  override.

All three call the amend-published confirmation guard. Amend variants' shared
argument assertion accepts staged changes; unstaged changes only when `--all`
is present; and otherwise permits amend/allow-empty/reset-author/signoff/author/
date options because these can validly change metadata without a patch. With
none of those conditions Magit errors “Nothing staged (or unstaged)”. `--all`
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
