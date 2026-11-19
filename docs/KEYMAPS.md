## 5. Default keymaps (profiles)

Profiles are data files shipped in the binary (`profiles/standard.toml`,
`profiles/vim.toml`) and validated by tests. Values below are **proposals**
meant to be committed as the first version; edit freely.

**Rules for every binding in every profile**
- R1. No Emacs prefix keys (`ctrl+c ctrl+*`, `ctrl+x …`) and no `alt+<letter>`
  meant to mimic `M-<letter>`.
- R2. Prefer Magit's mnemonic letters where the letter matches the Git
  operation name (`c` commit, `b` branch, `l` log, `d` diff, `f` fetch,
  `F` pull, `P` push, `m` merge, `r` rebase, `t` tag, `s`/`u` stage/unstage).
- R3. `standard` uses direct, widely-known keys: `Enter` confirms, `Escape`
  cancels, `Delete` discards, arrows navigate.
- R4. `vim` never rebinds visual-mode keys (`v`, `V`, `ctrl+v`), counts
  (`0`–`9` as counts), `j k`, `gg`, `G`, `ctrl+d/u/f/b`, `/`, `H`, `L`.
  Structured views may bind `h`/`l`; text mode retains native horizontal motions.
- R5. Every command has a binding (or an explicit "unbound" entry) in every
  profile; a test enforces this.
- R6. `ctrl+c` always means cancel in non-editable views (with the
  copy-passthrough rule from §3.9). Editable scopes use the keys below.

### 5.1 `common` scope

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `Enter` | `Enter` |
| `common.cancel` | `Escape`, `ctrl+c` | `Escape`, `ctrl+c` |
| `common.quit` | `q` | `q` |
| `common.refresh` | `g`, `F5` | `g r` |
| `common.toggle-section` | `Tab` | `Tab`, `z a` |
| `common.show-section` / `hide-section` | — | `z o` / `z c` |
| `common.cycle-visibility` (all sections) | `shift+Tab` | `shift+Tab` |
| `common.show-all` / `hide-all` | unbound | `z R` / `z M` |
| `common.more-detail` / `less-detail` | unbound | `z r` / `z m` |
| `common.detail-level-1` … `detail-level-4` | `alt+1` … `alt+4` | unbound |
| `common.next-section` / `prev-section` | `n` / `p` | `] ]` / `[ [` |
| `common.next-sibling` / `prev-sibling` | `alt+ArrowDown` / `alt+ArrowUp` | `g j` / `g k` |
| `common.parent-section` | `alt+ArrowLeft` | `g u` |
| `common.toggle-text-mode` | `ctrl+t` | `ctrl+t` |
| `common.copy-value` | `ctrl+shift+c` | `y y` |
| `common.visit` | `Enter` | `Enter` |
| `open.dispatch` (help / menu of menus) | `?` | `g ?` |

Visibility has four levels: section headers → file headers → hunk headers
(without body lines) → individually expanded hunk bodies. The four
`common.detail-level-N` actions set an absolute level; `standard` binds
`alt+1` … `alt+4` to these actions. `vim` uses `z m`/`z r` to step down/up
and `z M`/`z R` to hide/show extremes. These are server-owned visibility
actions; clients do not infer structure or key-specific state.

The four R-NAV bindings are Implemented. `shift+Tab` cycles the global detail
level 4 → 1 → 2 → 3 → 4. Sibling motion stays within the immediate parent
(root sections are siblings), never wraps, and treats a diff body as its hunk.
Parent motion goes to the immediate containing header; roots stay put.

### 5.2 `status` scope (also used by diff-like views for staging)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `status.stage` | `s` | `s` |
| `status.unstage` | `u` | `u` |
| `status.stage-all` | `S` | `S` |
| `status.unstage-all` | `U` | `U` |
| `status.discard` | `Delete` | `x` |
| `status.reverse` | `v` | `R` |
| `status.apply` | `a` | `a` |
| `open.commit` | `c` | `c` |
| `open.branch` | `b` | `b` |
| `open.log` | `l` | `l` |
| `open.diff` | `d` | `d` |
| `open.fetch` | `f` | `f` |
| `open.pull` | `F` | `F` |
| `open.push` | `P` | `P` |
| `open.merge` | `m` | `m` |
| `open.rebase` | `r` | `r` |
| `open.stash` | `z` | `Z` |
| `open.tag` | `t` | `t` |
| `open.reset` | `x` | `X` |
| `open.revert` | `V` | `-` |
| `open.cherry-pick` | `A` | `A` |
| `open.bisect` | `B` | `B` |
| `open.remote` | `M` | `M` |
| `open.submodule` | `o` | `o` |
| `open.worktree` | `%` | `g %` |
| `open.ignore` | `i` | `i` |
| `open.refs` | `y` | `g y` |
| `open.cherries` | `Y` | `g Y` |
| `open.patch-apply` | `w` | `w` |
| `open.patch-create` | `W` | `W` |
| `open.run` | `!` | `!` |
| `open.process-log` | `$` | `g $` |

`vim` keeps the single letters `f F t T m r M` (shadowing the Vim find/till/
mark/replace motions) because they match Magit and are useless in a read-only
buffer. `v V ctrl+v` remain Vim's visual modes, which is why reverse is `R`,
revert is `-`, stash is `Z` (since `z` is the fold prefix), reset is `X`
(since `x` is discard) and a few menu keys gain a `g` prefix.

### 5.3 Select mode (`log.select`, `refs.select`)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `Enter` | `Enter` |
| `select.toggle-mark` (multi) | `Space` | `Space` |
| `select.enter-rev` | `i` | `i` |
| `common.cancel` | `Escape`, `ctrl+c` | `Escape`, `ctrl+c` |

### 5.4 Menu scopes (`menu.*`)

| Command | `standard` | `vim` |
| --- | --- | --- |
| Toggle a switch | `-` then its letter | same |
| Set an option | `=` then its letter | same |
| Run an action | its letter | same |
| `common.cancel` / close | `Escape`, `ctrl+c`, `q` | same |
| `menu.save-args` | `ctrl+s` | `ctrl+s` |
| `menu.reset-args` | `ctrl+r` | `ctrl+r` |
| `open.dispatch` (toggle help) | `?` | `?` |

#### 5.4.1 Log menu: supported subset

Open the log menu with `l` from status. These menu keys are identical in both
profiles; set filters before choosing an action.

| Keys | Supported action or filter |
| --- | --- |
| `l` | Current history (`HEAD`) |
| `L` | Local branches (`--branches`) |
| `b` | All local and remote-tracking branches (`--branches --remotes`) |
| `a` | All references (`--all`, including tags) |
| `c` | Select and copy commit IDs |
| `= n` | Commit limit |
| `= a`, `= s`, `= u` | Author, since, until (Git date expressions) |
| `= g`, `= S`, `= G`, `= f` | Message grep, string/regex pickaxe, literal path |
| `= M`, `= m`, `= p` | Toggle merges only, omit merges, first-parent traversal |
| `- g`, `- d` | Toggle graph and ref decorations |

Filters compose with each supported history variant. Other displayed log menu
entries remain planned, including reflogs, other/related revisions and matching
refs. In a normal log, `/` and motion keys stay native to Neovim; `q` returns to
the caller. No new revision or reflog view is implied by these variants.

#### 5.4.2 Branch menu: supported subset

Open the branch menu with `b` from status. These menu keys are identical in
both profiles.

| Keys | Supported action |
| --- | --- |
| `n` | Create a branch at `HEAD` without switching to it |
| `m` | Rename a branch: prompt for the source, then the new name |
| `k` | Delete a branch, leaving Git's refusal of the checked-out branch as a message |
| `d` | Set a branch description: prompt for the branch, then for its text |
| `x` | Reset a branch to a revision: prompt for the branch, then the target revision; a non-current branch is force-moved, while the current branch is hard-reset, confirming when the working tree is dirty |

Branch checkout, the `b C` configure popup and worktree entries remain planned.
Resetting a non-current branch force-moves it without checking it out. Resetting
the current branch hard-resets it, so uncommitted changes are discarded after
confirmation.
Creating a branch leaves the current branch checked out; renaming the checked-out
branch renames it in place.

#### 5.4.3 Tag menu: supported subset

Open the tag menu with `t` from status. These menu keys are identical in both
profiles.

| Keys | Supported action |
| --- | --- |
| `t` | Create a tag at `HEAD` |
| `k` | Delete a tag, leaving Git's refusal of a missing tag as a message |

Tag release and prune remain planned.

#### 5.4.4 Remote menu: supported subset

Open the remote menu with `M` from status. These menu keys are identical in both
profiles.

| Keys | Supported action |
| --- | --- |
| `a` | Add a remote: prompt for the name, then for its URL |
| `r` | Rename a remote: prompt for the current name, then for the new name |
| `k` | Remove a remote, leaving Git's refusal of a missing remote as a message |
| `p` | Prune stale branches of a remote: prompt for its name, then delete local tracking refs the remote no longer has |

Remote configure, prune-refspecs, unshallow and update-default remain planned.

#### 5.4.5 Reset menu: supported subset

Open the reset menu with `x` (bound to `X` in the `vim` profile). These menu keys
are identical in both profiles.

| Keys | Supported action |
| --- | --- |
| `X m` | Mixed reset: move `HEAD` and reset the index to a prompted revision, leaving the working tree alone |
| `X s` | Soft reset: move `HEAD` to a prompted revision, leaving index and working tree alone |
| `X h` | Hard reset: move `HEAD` and reset the index and working tree to a prompted revision, confirming when the working tree is dirty |
| `X k` | Keep reset: move `HEAD` and reset the index to a prompted revision, keeping uncommitted changes intact |
| `X b` | Reset a branch: prompt for the branch, then the target revision; identical to `b x`, including the current-branch hard reset and its dirty-tree confirmation |

Each row prompts once for the target revision; a free-text target never reaches
Git in option position. `X h` asking for confirmation on a dirty tree is a
deliberate deviation from Magit, which hard-resets without asking. `X f/i/w`
(path-scoped) remain planned.

### 5.5 Commit-message scope

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `ctrl+Enter` | `Z Z` (and `ctrl+Enter` where the terminal supports it) |
| `common.cancel` | `ctrl+shift+x` | `Z Q` |
| `commit-message.history-prev` / `next` | `alt+ArrowUp` / `alt+ArrowDown` | `[ m` / `] m` (normal mode) |
| `commit-message.add-signoff` | `ctrl+alt+s` | `g s` |

Closing the buffer confirms the saved disk text; it does not cancel (§3.10).
Use the mapped `common.cancel` key to cancel. `ctrl+c` is **not** cancel here
because it is copy in non-modal editors.
The current bridge emits this scope for commit messages; merge/tag message
support is not emitted yet.

The commit menu's `=S` option selects Git commit signing. An empty value passes
`--gpg-sign=` and asks Git to use the committer's default signing key; a
non-empty value selects that key. Signing is performed by the user's Git
signing backend/agent. Configure its key and pinentry in Git and the signing
environment; this client does not collect passphrases, create a terminal, or
configure loopback pinentry. Interactive pinentry requires a working user
environment (for example, an appropriate `GPG_TTY` in terminal sessions).
See [`git commit --gpg-sign`](https://git-scm.com/docs/git-commit#Documentation/git-commit.txt---gpg-signltkey-idgt)
and [GnuPG agent invocation](https://www.gnupg.org/documentation/manuals/gnupg/Invoking-GPG_002dAGENT.html).

### 5.6 Rebase todo (structured view)

The rebase menu's `i` action (`rebase.interactive`) opens a single-base
selector; confirming a base opens the prepared initial todo. It does not run Git.

| Command | `standard` | `vim` |
| --- | --- | --- |
| `rebase.todo-submit` | `ctrl+s` | `ctrl+s` |
| `rebase.todo-pick` / `rebase.todo-reword` / `rebase.todo-edit` | `p` / `r` / `e` | `p` / `r` / `e` |
| `rebase.todo-squash` / `rebase.todo-fixup` / `rebase.todo-drop` | `s` / `f` / `d` | `s` / `f` / `d` |
| `rebase.todo-up` / `rebase.todo-down` | `K` / `J` | `K` / `J` |
| `rebase.todo-set-action` | `view/invoke` with `{"action":"pick|reword|edit|squash|fixup|drop"}` at selected commit | same |
| `rebase.todo-move-before` | `view/invoke` with `{"before":"<full OID>"}` at selected commit; null moves to end | same |
| `common.cancel` | `Escape` / `ctrl+c` | `Escape` / `ctrl+c` |

(Other views define their bindings when implemented; they must follow R1–R6.)

---
