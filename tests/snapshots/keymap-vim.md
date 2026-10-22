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
| `common.quit` | `q` | planned | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | planned | Toggle text mode |
| `common.copy-value` | `y y` | planned | Copy the value at point |
| `open.dispatch` | `g ?` | planned | Show the dispatch menu |

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
| `common.quit` | `q` | planned | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | planned | Toggle text mode |
| `common.copy-value` | `y y` | planned | Copy the value at point |
| `common.visit` | `Enter` | planned | Visit the thing at point |
| `status.discard` | `x` | planned | Discard the change at point |
| `status.reverse` | `R` | planned | Reverse-apply the change at point |
| `status.apply` | `a` | planned | Apply the change at point |
| `open.dispatch` | `g ?` | planned | Show the dispatch menu |
| `open.commit` | `c` | planned | Open the commit menu |
| `open.branch` | `b` | planned | Open the branch menu |
| `open.log` | `g l` | planned | Open the log menu |
| `open.diff` | `d` | planned | Open the diff menu |
| `open.fetch` | `f` | planned | Open the fetch menu |
| `open.pull` | `F` | planned | Open the pull menu |
| `open.push` | `P` | planned | Open the push menu |
| `open.merge` | `m` | planned | Open the merge menu |
| `open.rebase` | `r` | planned | Open the rebase menu |
| `open.stash` | `Z` | planned | Open the stash menu |
| `open.tag` | `t` | planned | Open the tag menu |
| `open.reset` | `X` | planned | Open the reset menu |
| `open.revert` | `-` | planned | Open the revert menu |
| `open.cherry-pick` | `A` | planned | Open the cherry-pick menu |
| `open.bisect` | `B` | planned | Open the bisect menu |
| `open.remote` | `M` | planned | Open the remote menu |
| `open.submodule` | `o` | planned | Open the submodule menu |
| `open.worktree` | `g %` | planned | Open the worktree menu |
| `open.ignore` | `i` | planned | Open the ignore menu |
| `open.refs` | `g y` | planned | Open the refs view |
| `open.cherries` | `g Y` | planned | Open the cherry view |
| `open.patch-apply` | `w` | planned | Apply a patch |
| `open.patch-create` | `W` | planned | Create a patch |
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
| `common.quit` | `q` | planned | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | planned | Toggle text mode |
| `common.copy-value` | `y y` | planned | Copy the value at point |
| `open.dispatch` | `g ?` | planned | Show the dispatch menu |
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
| `common.quit` | `q` | planned | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | planned | Toggle text mode |
| `common.copy-value` | `y y` | planned | Copy the value at point |
| `open.dispatch` | `g ?` | planned | Show the dispatch menu |
| `menu.save-args` | `ctrl+s` | planned | Save the menu arguments |
| `menu.reset-args` | `ctrl+r` | planned | Reset the menu arguments |

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
| `common.quit` | `q` | planned | Close all views |
| `common.cycle-visibility` | `shift+Tab` | planned | Cycle the visibility of every section |
| `common.next-sibling` | `g j` | planned | Move to the next sibling |
| `common.prev-sibling` | `g k` | planned | Move to the previous sibling |
| `common.parent-section` | `g u` | planned | Move to the parent section |
| `common.toggle-text-mode` | `ctrl+t` | planned | Toggle text mode |
| `common.copy-value` | `y y` | planned | Copy the value at point |
| `open.dispatch` | `g ?` | planned | Show the dispatch menu |
| `commit-message.history-prev` | `[ m` | planned | Recall the previous commit message |
| `commit-message.history-next` | `] m` | planned | Recall the next commit message |
| `commit-message.add-signoff` | `g s` | planned | Add a Signed-off-by trailer |

