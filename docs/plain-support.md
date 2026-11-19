# Plain presentation command support

This table is generated from command registry metadata (`Command::effects`, `Command::scope` and `Command::status`) and `PlainCapabilities::plain_availability`; it records declarations, not the result of executing each command. The `minimal`, `edits` and `full` columns evaluate three representative capability sets. Effect declarations are enforced when effects are delivered, so an available command still needs the listed capability before an optional effect reaches the client. Do not hand-edit this file; regenerate it with `make registry-docs-generate`.

| Command | Scope | Status | Effects | Needs capability | minimal | edits | full | Reason |
|---|---|---|---|---|---|---|---|---|
| `bisect.good` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `bisect.mark` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `bisect.reset` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `bisect.run` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `bisect.skip` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `bisect.start` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `blame.cycle-style` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `blame.echo` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `blame.next-chunk` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `blame.next-chunk-same-commit` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `blame.previous-chunk` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `blame.previous-chunk-same-commit` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `blame.removal` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `blame.reverse` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `blame.start` | menu-action | Planned (R-8.1) | — | — | Planned (R-8.1) | Planned (R-8.1) | Planned (R-8.1) | — |
| `branch.checkout` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.checkout-local` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.checkout-remote` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.configure` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.configure-description` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `branch.create` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `branch.create-and-checkout` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.delete` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `branch.orphan` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.rename` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `branch.reset` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `branch.shelve` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.spinoff` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.spinout` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.unshelve` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.update-default` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.worktree-checkout` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `branch.worktree-create` | menu-action | Planned (R-6.1) | — | — | Planned (R-6.1) | Planned (R-6.1) | Planned (R-6.1) | — |
| `cherry-pick.apply` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `cherry-pick.donate` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `cherry-pick.harvest` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `cherry-pick.pick` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `cherry-pick.spinoff` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `cherry-pick.spinout` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `cherry-pick.squash` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `commit-message.add-signoff` | commit-message | Planned (R-4.2) | — | — | Planned (R-4.2) | Planned (R-4.2) | Planned (R-4.2) | — |
| `commit-message.history-next` | commit-message | Planned (R-EDIT-HISTORY) | — | — | Planned (R-EDIT-HISTORY) | Planned (R-EDIT-HISTORY) | Planned (R-EDIT-HISTORY) | — |
| `commit-message.history-prev` | commit-message | Planned (R-EDIT-HISTORY) | — | — | Planned (R-EDIT-HISTORY) | Planned (R-EDIT-HISTORY) | Planned (R-EDIT-HISTORY) | — |
| `commit.absorb-modules` | menu-action | Planned (R-4.1) | — | — | Planned (R-4.1) | Planned (R-4.1) | Planned (R-4.1) | — |
| `commit.alter` | menu-action | Planned (R-4.1) | — | — | Planned (R-4.1) | Planned (R-4.1) | Planned (R-4.1) | — |
| `commit.amend` | menu-action | Implemented | editDocument | `workspace.applyEdit` | Unavailable — workspace.applyEdit is required | Available | Available | workspace.applyEdit is required |
| `commit.augment` | menu-action | Planned (R-4.1) | — | — | Planned (R-4.1) | Planned (R-4.1) | Planned (R-4.1) | — |
| `commit.autofixup` | menu-action | Planned (R-4.1) | — | — | Planned (R-4.1) | Planned (R-4.1) | Planned (R-4.1) | — |
| `commit.create` | menu-action | Implemented | editDocument | `workspace.applyEdit` | Unavailable — workspace.applyEdit is required | Available | Available | workspace.applyEdit is required |
| `commit.extend` | menu-action | Implemented | editDocument | `workspace.applyEdit` | Unavailable — workspace.applyEdit is required | Available | Available | workspace.applyEdit is required |
| `commit.fixup` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `commit.instant-fixup` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `commit.instant-squash` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `commit.reshelve` | menu-action | Planned (R-4.1) | — | — | Planned (R-4.1) | Planned (R-4.1) | Planned (R-4.1) | — |
| `commit.revise` | menu-action | Planned (R-4.1) | — | — | Planned (R-4.1) | Planned (R-4.1) | Planned (R-4.1) | — |
| `commit.reword` | menu-action | Implemented | editDocument | `workspace.applyEdit` | Unavailable — workspace.applyEdit is required | Available | Available | workspace.applyEdit is required |
| `commit.reword-past` | menu-action | Planned (R-4.1) | — | — | Planned (R-4.1) | Planned (R-4.1) | Planned (R-4.1) | — |
| `commit.squash` | menu-action | Planned (R-4.1) | — | — | Planned (R-4.1) | Planned (R-4.1) | Planned (R-4.1) | — |
| `common.cancel` | common | Implemented | — | — | Available | Available | Available | — |
| `common.confirm` | common | Implemented | — | — | Available | Available | Available | — |
| `common.copy-value` | common | Implemented | clipboard | none (Plain) | Unavailable — the clipboard effect is not available in Plain mode | Unavailable — the clipboard effect is not available in Plain mode | Unavailable — the clipboard effect is not available in Plain mode | the clipboard effect is not available in Plain mode |
| `common.cycle-visibility` | common | Implemented | — | — | Available | Available | Available | — |
| `common.detail-level-1` | common | Implemented | — | — | Available | Available | Available | — |
| `common.detail-level-2` | common | Implemented | — | — | Available | Available | Available | — |
| `common.detail-level-3` | common | Implemented | — | — | Available | Available | Available | — |
| `common.detail-level-4` | common | Implemented | — | — | Available | Available | Available | — |
| `common.hide-all` | common | Implemented | — | — | Available | Available | Available | — |
| `common.hide-section` | common | Implemented | — | — | Available | Available | Available | — |
| `common.jump` | common | Implemented | — | — | Available | Available | Available | — |
| `common.less-detail` | common | Implemented | — | — | Available | Available | Available | — |
| `common.more-detail` | common | Implemented | — | — | Available | Available | Available | — |
| `common.next-section` | common | Implemented | — | — | Available | Available | Available | — |
| `common.next-sibling` | common | Implemented | — | — | Available | Available | Available | — |
| `common.parent-section` | common | Implemented | — | — | Available | Available | Available | — |
| `common.prev-section` | common | Implemented | — | — | Available | Available | Available | — |
| `common.prev-sibling` | common | Implemented | — | — | Available | Available | Available | — |
| `common.quit` | common | Implemented | — | — | Available | Available | Available | — |
| `common.refresh` | common | Implemented | — | — | Available | Available | Available | — |
| `common.show-all` | common | Implemented | — | — | Available | Available | Available | — |
| `common.show-section` | common | Implemented | — | — | Available | Available | Available | — |
| `common.toggle-section` | common | Implemented | — | — | Available | Available | Available | — |
| `common.toggle-text-mode` | common | Implemented | — | — | Available | Available | Available | — |
| `common.visit` | common | Implemented | visitFile | `window.showDocument.support` | Unavailable — window.showDocument.support is required | Unavailable — window.showDocument.support is required | Available | window.showDocument.support is required |
| `diff.dwim` | menu-action | Planned (R-5.2) | — | — | Planned (R-5.2) | Planned (R-5.2) | Planned (R-5.2) | — |
| `diff.paths` | menu-action | Planned (R-5.2) | — | — | Planned (R-5.2) | Planned (R-5.2) | Planned (R-5.2) | — |
| `diff.range` | menu-action | Planned (R-5.2) | — | — | Planned (R-5.2) | Planned (R-5.2) | Planned (R-5.2) | — |
| `diff.show-commit` | menu-action | Planned (R-5.2) | — | — | Planned (R-5.2) | Planned (R-5.2) | Planned (R-5.2) | — |
| `diff.show-stash` | menu-action | Planned (R-5.2) | — | — | Planned (R-5.2) | Planned (R-5.2) | Planned (R-5.2) | — |
| `diff.staged` | menu-action | Planned (R-5.2) | — | — | Planned (R-5.2) | Planned (R-5.2) | Planned (R-5.2) | — |
| `diff.unstaged` | menu-action | Planned (R-5.2) | — | — | Planned (R-5.2) | Planned (R-5.2) | Planned (R-5.2) | — |
| `diff.worktree` | menu-action | Planned (R-5.2) | — | — | Planned (R-5.2) | Planned (R-5.2) | Planned (R-5.2) | — |
| `fetch.all` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `fetch.branch` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `fetch.configure` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `fetch.from-pushremote` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `fetch.from-upstream` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `fetch.modules` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `fetch.other` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `fetch.refspec` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `ignore.assume-unchanged` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `ignore.gitdir` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `ignore.no-assume-unchanged` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `ignore.no-skip-worktree` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `ignore.skip-worktree` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `ignore.subdir` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `ignore.system` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `ignore.topdir` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `log.all-branches` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `log.all-references` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `log.current` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `log.head` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.load-more` | log | Implemented | — | — | Available | Available | Available | — |
| `log.local-branches` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `log.matching-branches` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.matching-tags` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.merged` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.other` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.reflog-current` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.reflog-head` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.reflog-objects` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.reflog-other` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.related` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.select-and-copy` | menu-action | Implemented | clipboard | none (Plain) | Unavailable — the clipboard effect is not available in Plain mode | Unavailable — the clipboard effect is not available in Plain mode | Unavailable — the clipboard effect is not available in Plain mode | the clipboard effect is not available in Plain mode |
| `log.shortlog` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.wiplog-index` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `log.wiplog-worktree` | menu-action | Planned (R-5.1) | — | — | Planned (R-5.1) | Planned (R-5.1) | Planned (R-5.1) | — |
| `menu.reset-args` | menu | Implemented | — | — | Available | Available | Available | — |
| `menu.save-args` | menu | Implemented | — | — | Available | Available | Available | — |
| `merge.absorb` | menu-action | Planned (R-7.1) | — | — | Planned (R-7.1) | Planned (R-7.1) | Planned (R-7.1) | — |
| `merge.dissolve` | menu-action | Planned (R-7.1) | — | — | Planned (R-7.1) | Planned (R-7.1) | Planned (R-7.1) | — |
| `merge.editmsg` | menu-action | Planned (R-7.1) | — | — | Planned (R-7.1) | Planned (R-7.1) | Planned (R-7.1) | — |
| `merge.nocommit` | menu-action | Planned (R-7.1) | — | — | Planned (R-7.1) | Planned (R-7.1) | Planned (R-7.1) | — |
| `merge.plain` | menu-action | Planned (R-7.1) | — | — | Planned (R-7.1) | Planned (R-7.1) | Planned (R-7.1) | — |
| `merge.preview` | menu-action | Planned (R-7.1) | — | — | Planned (R-7.1) | Planned (R-7.1) | Planned (R-7.1) | — |
| `merge.squash` | menu-action | Planned (R-7.1) | — | — | Planned (R-7.1) | Planned (R-7.1) | Planned (R-7.1) | — |
| `open.bisect` | status | Implemented | — | — | Available | Available | Available | — |
| `open.branch` | status | Implemented | — | — | Available | Available | Available | — |
| `open.cherries` | status | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `open.cherry-pick` | status | Implemented | — | — | Available | Available | Available | — |
| `open.commit` | status | Implemented | — | — | Available | Available | Available | — |
| `open.diff` | status | Implemented | — | — | Available | Available | Available | — |
| `open.dispatch` | common | Implemented | — | — | Available | Available | Available | — |
| `open.fetch` | status | Implemented | — | — | Available | Available | Available | — |
| `open.ignore` | status | Implemented | — | — | Available | Available | Available | — |
| `open.log` | status | Implemented | — | — | Available | Available | Available | — |
| `open.merge` | status | Implemented | — | — | Available | Available | Available | — |
| `open.patch-apply` | status | Implemented | — | — | Available | Available | Available | — |
| `open.patch-create` | status | Implemented | — | — | Available | Available | Available | — |
| `open.process-log` | status | Implemented | — | — | Available | Available | Available | — |
| `open.pull` | status | Implemented | — | — | Available | Available | Available | — |
| `open.push` | status | Implemented | — | — | Available | Available | Available | — |
| `open.rebase` | status | Implemented | — | — | Available | Available | Available | — |
| `open.refs` | status | Planned (R-5.3) | — | — | Planned (R-5.3) | Planned (R-5.3) | Planned (R-5.3) | — |
| `open.remote` | status | Implemented | — | — | Available | Available | Available | — |
| `open.reset` | status | Implemented | — | — | Available | Available | Available | — |
| `open.revert` | status | Implemented | — | — | Available | Available | Available | — |
| `open.run` | status | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `open.stash` | status | Implemented | — | — | Available | Available | Available | — |
| `open.submodule` | status | Implemented | — | — | Available | Available | Available | — |
| `open.tag` | status | Implemented | — | — | Available | Available | Available | — |
| `open.worktree` | status | Implemented | — | — | Available | Available | Available | — |
| `patch.apply-maildir` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `patch.apply-patches` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `patch.apply-plain` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `patch.create` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `patch.request-pull` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `patch.save` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `process-log.clear` | process-log | Implemented | — | — | Available | Available | Available | — |
| `pull.fetch-remotes` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `pull.fetch-remotes-prune` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `pull.from-pushremote` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `pull.from-upstream` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `pull.into-upstream` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `pull.other` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.configure` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.current` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.matching` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.notes-ref` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.other` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.refspecs` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.tag` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.tags` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.to-pushremote` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `push.to-upstream` | menu-action | Planned (R-6.2) | — | — | Planned (R-6.2) | Planned (R-6.2) | Planned (R-6.2) | — |
| `rebase.abort` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `rebase.autosquash` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.change-dates` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.continue` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `rebase.edit-commit` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.edit-todo` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `rebase.elsewhere` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.interactive` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `rebase.onto-pushremote` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.onto-upstream` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.remove-commit` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.reword-commit` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.skip` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `rebase.subset` | menu-action | Planned (R-7.3) | — | — | Planned (R-7.3) | Planned (R-7.3) | Planned (R-7.3) | — |
| `rebase.todo-down` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-drop` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-edit` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-fixup` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-move-before` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-pick` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-reword` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-set-action` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-squash` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-submit` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `rebase.todo-up` | rebase-todo | Implemented | — | — | Available | Available | Available | — |
| `remote.add` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `remote.configure` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `remote.prune` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `remote.prune-refspecs` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `remote.remove` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `remote.rename` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `remote.unshallow` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `remote.update-default` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `reset.branch` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `reset.file` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `reset.hard` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `reset.index` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `reset.keep` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `reset.mixed` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `reset.soft` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `reset.worktree` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `revert.changes` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `revert.commit` | menu-action | Planned (R-7.2) | — | — | Planned (R-7.2) | Planned (R-7.2) | Planned (R-7.2) | — |
| `select.enter-rev` | select | Implemented | — | — | Available | Available | Available | — |
| `select.toggle-mark` | select | Implemented | — | — | Available | Available | Available | — |
| `stash.apply` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.both` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.branch` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.branch-here` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.drop` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.format-patch` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.index` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.keep-index` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.list` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.pop` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.push` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.show` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.snapshot-both` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.snapshot-index` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.snapshot-worktree` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.wip-commit` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `stash.worktree` | menu-action | Planned (R-6.5) | — | — | Planned (R-6.5) | Planned (R-6.5) | Planned (R-6.5) | — |
| `status.apply` | status | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `status.discard` | status | Implemented | — | — | Unavailable — workspace.applyEdit and textDocument.codeAction are required | Available | Available | workspace.applyEdit and textDocument.codeAction are required |
| `status.reverse` | status | Planned (R-3.6) | — | — | Planned (R-3.6) | Planned (R-3.6) | Planned (R-3.6) | — |
| `status.stage` | status | Implemented | — | — | Unavailable — workspace.applyEdit and textDocument.codeAction are required | Available | Available | workspace.applyEdit and textDocument.codeAction are required |
| `status.stage-all` | status | Implemented | — | — | Available | Available | Available | — |
| `status.unstage` | status | Implemented | — | — | Unavailable — workspace.applyEdit and textDocument.codeAction are required | Available | Available | workspace.applyEdit and textDocument.codeAction are required |
| `status.unstage-all` | status | Implemented | — | — | Available | Available | Available | — |
| `submodule.add` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `submodule.fetch` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `submodule.list` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `submodule.populate` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `submodule.register` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `submodule.remove` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `submodule.synchronize` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `submodule.unpopulate` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `submodule.update` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `tag.create` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `tag.delete` | menu-action | Implemented | — | — | Available | Available | Available | — |
| `tag.prune` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `tag.release` | menu-action | Planned (R-6.3) | — | — | Planned (R-6.3) | Planned (R-6.3) | Planned (R-6.3) | — |
| `worktree.checkout` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `worktree.create` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `worktree.delete` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `worktree.move` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
| `worktree.visit` | menu-action | Planned (R-8.2) | — | — | Planned (R-8.2) | Planned (R-8.2) | Planned (R-8.2) | — |
