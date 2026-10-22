# magit-lsp keymap: vim

## common

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `g r` | implemented | Refresh the view |
| `common.toggle-section` | `Tab`, `z a` | implemented | Fold or unfold the section at point |
| `common.show-section` | `z o` | implemented | Unfold the section at point |
| `common.hide-section` | `z c` | implemented | Fold the section at point |
| `common.show-all` | `z R` | implemented | Unfold every section |
| `common.hide-all` | `z M` | implemented | Fold every section |
| `common.more-detail` | `z r` | implemented | Show one more level of detail |
| `common.less-detail` | `z m` | implemented | Show one less level of detail |
| `common.next-section` | `] ]` | implemented | Move to the next section |
| `common.prev-section` | `[ [` | implemented | Move to the previous section |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | `Enter` | planned | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `y y` | implemented | Copy the value at point |
| `open.dispatch` | `g ?` | implemented | Show the dispatch menu |

## status

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `g r` | implemented | Refresh the view |
| `common.toggle-section` | `Tab`, `z a` | implemented | Fold or unfold the section at point |
| `common.show-section` | `z o` | implemented | Unfold the section at point |
| `common.hide-section` | `z c` | implemented | Fold the section at point |
| `common.show-all` | `z R` | implemented | Unfold every section |
| `common.hide-all` | `z M` | implemented | Fold every section |
| `common.more-detail` | `z r` | implemented | Show one more level of detail |
| `common.less-detail` | `z m` | implemented | Show one less level of detail |
| `common.next-section` | `] ]` | implemented | Move to the next section |
| `common.prev-section` | `[ [` | implemented | Move to the previous section |
| `status.stage` | `s` | implemented | Stage the change at point |
| `status.unstage` | `u` | implemented | Unstage the change at point |
| `status.stage-all` | `S` | implemented | Stage all unstaged changes |
| `status.unstage-all` | `U` | implemented | Unstage all staged changes |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | (unbound) | planned | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `y y` | implemented | Copy the value at point |
| `common.visit` | `Enter` | planned | Visit the thing at point |
| `status.discard` | `x` | planned | Discard the change at point |
| `status.reverse` | `R` | planned | Reverse-apply the change at point |
| `status.apply` | `a` | planned | Apply the change at point |
| `open.dispatch` | `g ?` | implemented | Show the dispatch menu |
| `open.commit` | `c` | implemented | Open the commit menu |
| `open.branch` | `b` | implemented | Open the branch menu |
| `open.log` | `g l` | implemented | Open the log menu |
| `open.diff` | `d` | implemented | Open the diff menu |
| `open.fetch` | `f` | implemented | Open the fetch menu |
| `open.pull` | `F` | implemented | Open the pull menu |
| `open.push` | `P` | implemented | Open the push menu |
| `open.merge` | `m` | implemented | Open the merge menu |
| `open.rebase` | `r` | implemented | Open the rebase menu |
| `open.stash` | `Z` | implemented | Open the stash menu |
| `open.tag` | `t` | implemented | Open the tag menu |
| `open.reset` | `X` | implemented | Open the reset menu |
| `open.revert` | `-` | implemented | Open the revert menu |
| `open.cherry-pick` | `A` | implemented | Open the cherry-pick menu |
| `open.bisect` | `B` | implemented | Open the bisect menu |
| `open.remote` | `M` | implemented | Open the remote menu |
| `open.submodule` | `o` | implemented | Open the submodule menu |
| `open.worktree` | `g %` | implemented | Open the worktree menu |
| `open.ignore` | `i` | implemented | Open the ignore menu |
| `open.patch-apply` | `w` | implemented | Open the apply-patches menu |
| `open.patch-create` | `W` | implemented | Open the format-patches menu |
| `open.refs` | `g y` | planned | Open the refs view |
| `open.cherries` | `g Y` | planned | Open the cherry view |
| `open.run` | `!` | planned | Run a Git command |
| `open.process-log` | `g $` | planned | Open the process log |

## select

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `g r` | implemented | Refresh the view |
| `common.toggle-section` | `Tab`, `z a` | implemented | Fold or unfold the section at point |
| `common.show-section` | `z o` | implemented | Unfold the section at point |
| `common.hide-section` | `z c` | implemented | Fold the section at point |
| `common.show-all` | `z R` | implemented | Unfold every section |
| `common.hide-all` | `z M` | implemented | Fold every section |
| `common.more-detail` | `z r` | implemented | Show one more level of detail |
| `common.less-detail` | `z m` | implemented | Show one less level of detail |
| `common.next-section` | `] ]` | implemented | Move to the next section |
| `common.prev-section` | `[ [` | implemented | Move to the previous section |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | `Enter` | planned | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `y y` | implemented | Copy the value at point |
| `open.dispatch` | `g ?` | implemented | Show the dispatch menu |
| `select.toggle-mark` | `Space` | planned | Toggle the mark on the thing at point |
| `select.enter-rev` | `i` | planned | Enter a revision by hand |

## menu

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `g r` | implemented | Refresh the view |
| `common.toggle-section` | `Tab`, `z a` | implemented | Fold or unfold the section at point |
| `common.show-section` | `z o` | implemented | Unfold the section at point |
| `common.hide-section` | `z c` | implemented | Fold the section at point |
| `common.show-all` | `z R` | implemented | Unfold every section |
| `common.hide-all` | `z M` | implemented | Fold every section |
| `common.more-detail` | `z r` | implemented | Show one more level of detail |
| `common.less-detail` | `z m` | implemented | Show one less level of detail |
| `common.next-section` | `] ]` | implemented | Move to the next section |
| `common.prev-section` | `[ [` | implemented | Move to the previous section |
| `common.cancel` | `Escape`, `ctrl+c` | implemented | Cancel the current operation |
| `common.confirm` | `Enter` | planned | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `y y` | implemented | Copy the value at point |
| `open.dispatch` | `g ?` | implemented | Show the dispatch menu |
| `menu.save-args` | `ctrl+s` | implemented | Save the menu arguments |
| `menu.reset-args` | `ctrl+r` | implemented | Reset the menu arguments |

## commit-message

| Command | Keys | Status | Description |
| --- | --- | --- | --- |
| `common.refresh` | `g r` | implemented | Refresh the view |
| `common.toggle-section` | `Tab`, `z a` | implemented | Fold or unfold the section at point |
| `common.show-section` | `z o` | implemented | Unfold the section at point |
| `common.hide-section` | `z c` | implemented | Fold the section at point |
| `common.show-all` | `z R` | implemented | Unfold every section |
| `common.hide-all` | `z M` | implemented | Fold every section |
| `common.more-detail` | `z r` | implemented | Show one more level of detail |
| `common.less-detail` | `z m` | implemented | Show one less level of detail |
| `common.next-section` | `] ]` | implemented | Move to the next section |
| `common.prev-section` | `[ [` | implemented | Move to the previous section |
| `common.cancel` | `Z Q` | implemented | Cancel the current operation |
| `common.confirm` | `Z Z`, `ctrl+Enter` | planned | Confirm the current selection |
| `common.quit` | `q` | implemented | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | implemented | Toggle text mode |
| `common.copy-value` | `y y` | implemented | Copy the value at point |
| `open.dispatch` | `g ?` | implemented | Show the dispatch menu |
| `commit-message.history-prev` | `[ m` | planned | Recall the previous commit message |
| `commit-message.history-next` | `] m` | planned | Recall the next commit message |
| `commit-message.add-signoff` | `g s` | planned | Add a Signed-off-by trailer |

