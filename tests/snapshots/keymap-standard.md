# magit-lsp keymap: standard

## common

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `F5`, `g` | implemented | Refresh the view |
| `common.toggle-section` | `Tab` | implemented | Fold or unfold the section at point |
| `common.show-section` | (unbound) | implemented | Unfold the section at point |
| `common.hide-section` | (unbound) | implemented | Fold the section at point |
| `common.show-all` | (unbound) | implemented | Unfold every section |
| `common.hide-all` | (unbound) | implemented | Fold every section |
| `common.more-detail` | (unbound) | implemented | Show one more level of detail |
| `common.less-detail` | (unbound) | implemented | Show one less level of detail |
| `common.detail-level-1` | `alt+1` | implemented | Show section headers only |
| `common.detail-level-2` | `alt+2` | implemented | Show section and file headers |
| `common.detail-level-3` | `alt+3` | implemented | Show hunk headers |
| `common.detail-level-4` | `alt+4` | implemented | Show hunk bodies |
| `common.next-section` | `n` | implemented | Move to the next section |
| `common.prev-section` | `p` | implemented | Move to the previous section |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | `Enter` | implemented | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `alt+ArrowDown` | planned | Move to the next sibling |
| `common.prev-sibling` | `alt+ArrowUp` | planned | Move to the previous sibling |
| `common.parent-section` | `alt+ArrowLeft` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `ctrl+shift+c` | implemented | Copy the value at point |
| `open.dispatch` | `?` | implemented | Show the dispatch menu |

## status

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `F5`, `g` | implemented | Refresh the view |
| `common.toggle-section` | `Tab` | implemented | Fold or unfold the section at point |
| `common.show-section` | (unbound) | implemented | Unfold the section at point |
| `common.hide-section` | (unbound) | implemented | Fold the section at point |
| `common.show-all` | (unbound) | implemented | Unfold every section |
| `common.hide-all` | (unbound) | implemented | Fold every section |
| `common.more-detail` | (unbound) | implemented | Show one more level of detail |
| `common.less-detail` | (unbound) | implemented | Show one less level of detail |
| `common.detail-level-1` | `alt+1` | implemented | Show section headers only |
| `common.detail-level-2` | `alt+2` | implemented | Show section and file headers |
| `common.detail-level-3` | `alt+3` | implemented | Show hunk headers |
| `common.detail-level-4` | `alt+4` | implemented | Show hunk bodies |
| `common.next-section` | `n` | implemented | Move to the next section |
| `common.prev-section` | `p` | implemented | Move to the previous section |
| `status.stage` | `s` | implemented | Stage the change at point |
| `status.unstage` | `u` | implemented | Unstage the change at point |
| `status.stage-all` | `S` | implemented | Stage all unstaged changes |
| `status.unstage-all` | `U` | implemented | Unstage all staged changes |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | (unbound) | implemented | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `alt+ArrowDown` | planned | Move to the next sibling |
| `common.prev-sibling` | `alt+ArrowUp` | planned | Move to the previous sibling |
| `common.parent-section` | `alt+ArrowLeft` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `ctrl+shift+c` | implemented | Copy the value at point |
| `common.visit` | `Enter` | planned | Visit the thing at point |
| `status.discard` | `Delete` | planned | Discard the change at point |
| `status.reverse` | `v` | planned | Reverse-apply the change at point |
| `status.apply` | `a` | planned | Apply the change at point |
| `open.dispatch` | `?`, `h` | implemented | Show the dispatch menu |
| `open.commit` | `c` | implemented | Open the commit menu |
| `open.branch` | `b` | implemented | Open the branch menu |
| `open.log` | `l` | implemented | Open the log menu |
| `open.diff` | `d` | implemented | Open the diff menu |
| `open.fetch` | `f` | implemented | Open the fetch menu |
| `open.pull` | `F` | implemented | Open the pull menu |
| `open.push` | `P` | implemented | Open the push menu |
| `open.merge` | `m` | implemented | Open the merge menu |
| `open.rebase` | `r` | implemented | Open the rebase menu |
| `open.stash` | `z` | implemented | Open the stash menu |
| `open.tag` | `t` | implemented | Open the tag menu |
| `open.reset` | `x` | implemented | Open the reset menu |
| `open.revert` | `V` | implemented | Open the revert menu |
| `open.cherry-pick` | `A` | implemented | Open the cherry-pick menu |
| `open.bisect` | `B` | implemented | Open the bisect menu |
| `open.remote` | `M` | implemented | Open the remote menu |
| `open.submodule` | `o` | implemented | Open the submodule menu |
| `open.worktree` | `%` | implemented | Open the worktree menu |
| `open.ignore` | `i` | implemented | Open the ignore menu |
| `open.patch-apply` | `w` | implemented | Open the apply-patches menu |
| `open.patch-create` | `W` | implemented | Open the format-patches menu |
| `open.refs` | `y` | planned | Open the refs view |
| `open.cherries` | `Y` | planned | Open the cherry view |
| `open.run` | `!` | planned | Run a Git command |
| `open.process-log` | `$` | implemented | Open the process log |

## select

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `F5`, `g` | implemented | Refresh the view |
| `common.toggle-section` | `Tab` | implemented | Fold or unfold the section at point |
| `common.show-section` | (unbound) | implemented | Unfold the section at point |
| `common.hide-section` | (unbound) | implemented | Fold the section at point |
| `common.show-all` | (unbound) | implemented | Unfold every section |
| `common.hide-all` | (unbound) | implemented | Fold every section |
| `common.more-detail` | (unbound) | implemented | Show one more level of detail |
| `common.less-detail` | (unbound) | implemented | Show one less level of detail |
| `common.detail-level-1` | `alt+1` | implemented | Show section headers only |
| `common.detail-level-2` | `alt+2` | implemented | Show section and file headers |
| `common.detail-level-3` | `alt+3` | implemented | Show hunk headers |
| `common.detail-level-4` | `alt+4` | implemented | Show hunk bodies |
| `common.next-section` | `n` | implemented | Move to the next section |
| `common.prev-section` | `p` | implemented | Move to the previous section |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | `Enter` | implemented | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `alt+ArrowDown` | planned | Move to the next sibling |
| `common.prev-sibling` | `alt+ArrowUp` | planned | Move to the previous sibling |
| `common.parent-section` | `alt+ArrowLeft` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `ctrl+shift+c` | implemented | Copy the value at point |
| `open.dispatch` | `?` | implemented | Show the dispatch menu |
| `select.toggle-mark` | `Space` | planned | Toggle the mark on the thing at point |
| `select.enter-rev` | `/` | planned | Enter a revision by hand |

## menu

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `F5`, `g` | implemented | Refresh the view |
| `common.toggle-section` | `Tab` | implemented | Fold or unfold the section at point |
| `common.show-section` | (unbound) | implemented | Unfold the section at point |
| `common.hide-section` | (unbound) | implemented | Fold the section at point |
| `common.show-all` | (unbound) | implemented | Unfold every section |
| `common.hide-all` | (unbound) | implemented | Fold every section |
| `common.more-detail` | (unbound) | implemented | Show one more level of detail |
| `common.less-detail` | (unbound) | implemented | Show one less level of detail |
| `common.detail-level-1` | `alt+1` | implemented | Show section headers only |
| `common.detail-level-2` | `alt+2` | implemented | Show section and file headers |
| `common.detail-level-3` | `alt+3` | implemented | Show hunk headers |
| `common.detail-level-4` | `alt+4` | implemented | Show hunk bodies |
| `common.next-section` | `n` | implemented | Move to the next section |
| `common.prev-section` | `p` | implemented | Move to the previous section |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | `Enter` | implemented | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `alt+ArrowDown` | planned | Move to the next sibling |
| `common.prev-sibling` | `alt+ArrowUp` | planned | Move to the previous sibling |
| `common.parent-section` | `alt+ArrowLeft` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `ctrl+shift+c` | implemented | Copy the value at point |
| `open.dispatch` | `?` | implemented | Show the dispatch menu |
| `menu.save-args` | `ctrl+s` | implemented | Save the menu arguments |
| `menu.reset-args` | `ctrl+r` | implemented | Reset the menu arguments |

## commit-message

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `F5`, `g` | implemented | Refresh the view |
| `common.toggle-section` | `Tab` | implemented | Fold or unfold the section at point |
| `common.show-section` | (unbound) | implemented | Unfold the section at point |
| `common.hide-section` | (unbound) | implemented | Fold the section at point |
| `common.show-all` | (unbound) | implemented | Unfold every section |
| `common.hide-all` | (unbound) | implemented | Fold every section |
| `common.more-detail` | (unbound) | implemented | Show one more level of detail |
| `common.less-detail` | (unbound) | implemented | Show one less level of detail |
| `common.detail-level-1` | `alt+1` | implemented | Show section headers only |
| `common.detail-level-2` | `alt+2` | implemented | Show section and file headers |
| `common.detail-level-3` | `alt+3` | implemented | Show hunk headers |
| `common.detail-level-4` | `alt+4` | implemented | Show hunk bodies |
| `common.next-section` | `n` | implemented | Move to the next section |
| `common.prev-section` | `p` | implemented | Move to the previous section |
| `common.cancel` | `ctrl+shift+x` | implemented | Cancel the current operation |
| `common.confirm` | `ctrl+Enter` | implemented | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | (unbound) | planned | Move to the next sibling |
| `common.prev-sibling` | (unbound) | planned | Move to the previous sibling |
| `common.parent-section` | `alt+ArrowLeft` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `ctrl+shift+c` | implemented | Copy the value at point |
| `open.dispatch` | `?` | implemented | Show the dispatch menu |
| `commit-message.history-prev` | `alt+ArrowUp` | planned | Recall the previous commit message |
| `commit-message.history-next` | `alt+ArrowDown` | planned | Recall the next commit message |
| `commit-message.add-signoff` | `ctrl+alt+s` | planned | Add a Signed-off-by trailer |

## process-log

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `F5`, `g` | implemented | Refresh the view |
| `common.toggle-section` | `Tab` | implemented | Fold or unfold the section at point |
| `common.show-section` | (unbound) | implemented | Unfold the section at point |
| `common.hide-section` | (unbound) | implemented | Fold the section at point |
| `common.show-all` | (unbound) | implemented | Unfold every section |
| `common.hide-all` | (unbound) | implemented | Fold every section |
| `common.more-detail` | (unbound) | implemented | Show one more level of detail |
| `common.less-detail` | (unbound) | implemented | Show one less level of detail |
| `common.detail-level-1` | `alt+1` | implemented | Show section headers only |
| `common.detail-level-2` | `alt+2` | implemented | Show section and file headers |
| `common.detail-level-3` | `alt+3` | implemented | Show hunk headers |
| `common.detail-level-4` | `alt+4` | implemented | Show hunk bodies |
| `common.next-section` | `n` | implemented | Move to the next section |
| `common.prev-section` | `p` | implemented | Move to the previous section |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | `Enter` | implemented | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `alt+ArrowDown` | planned | Move to the next sibling |
| `common.prev-sibling` | `alt+ArrowUp` | planned | Move to the previous sibling |
| `common.parent-section` | `alt+ArrowLeft` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `ctrl+shift+c` | implemented | Copy the value at point |
| `open.dispatch` | `?` | implemented | Show the dispatch menu |
| `process-log.clear` | `c` | implemented | Clear the process log |

