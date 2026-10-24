# Magit menu-key audit

## Immutable source and method

The reference is Magit **v4.7.1**, released 2026-09-01. The annotated tag
`v4.7.1` resolves to commit
`659f89955cf60fe3d4326d881c412df06c69680d` (tag object
`bb79175d7c2ddc309b89f33b499c8f27d7965503`). All links below pin that commit.
The manual is `docs/magit.texi`; menu/map definitions are from `lisp/` at the
same commit. `magit-keys.txt` is the independently maintained reference: each
row records menu, literal key, upstream Magit command identity, and local
command identity. Multi-step `d u` is one literal key field, not split on
whitespace. The test compares the data against every static action in the
menu definitions in both directions and rejects missing, orphan/mismatched,
and duplicate rows. It does not build expected key data from that file.

## Coverage and exclusions

The TSV covers **192 static action occurrences in all 22 static menu
definitions**, including all nine blame actions. Duplicate upstream commands
in different menus are distinct occurrences. Switches/options are arguments,
not actions; `?` is the common dispatch opener injected by `menus::build`;
dispatch entries are generated from profile keymaps. These are intentionally
not fabricated as static Magit menu actions. The data records no reset `X`:
the v4.7.1 reset transient uses `b f m s h k i w`, and `X` is a top-level
Magit prefix, not a reset-menu suffix.

Blame is not represented as a transient-only parity case. The manual's
[Blaming section](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L5168)
documents entering blame and operations in a blame buffer, and the pinned
`magit-blame.el` defines both the `magit-blame` transient (entry actions) and
the `magit-blame-read-only-mode-map` (buffer operations). The nine inventory
rows are sourced as follows: `b` = `magit-blame` / addition; `r` = removal;
`f` = reverse; `e` = echo, from the transient definition and corresponding
manual descriptions; `n`, `N`, `p`, `P` = next/previous chunk and same-commit
navigation, and `c` = cycle style, from the blame read-only mode map and
manual's blame-buffer bindings. They are cross-checked against the same-tag
definitions, not claimed to be nine entries in a blame transient. See pinned
[blame implementation](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-blame.el#L946-L964)
and its [blame keymaps](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-blame.el).

## State aliases (not extra static rows)

These are conditional Magit command aliases which reuse keys already present
in the static inventory. The test requires each alias to point at an observed
menu/key row. This is explanatory source parity, not an assertion that the
local adapter implements those transient-state commands.

| Menu/key | State and Magit command | Reason and pinned evidence |
| --- | --- | --- |
| rebase `s` | In-progress: `magit-rebase-skip` | The ordinary suffix is `magit-rebase-subset`; the in-progress map reuses `s` for Skip. [sequence definitions](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L524-L570) |
| rebase `e` | In-progress: `magit-rebase-edit` | The ordinary suffix is `magit-rebase-branch`; the in-progress map reuses `e` for Edit. Same pinned sequence definition. |
| bisect `B` | Active bisect: `magit-bisect-mark` (Bad) | `B` starts a bisect before one is active and marks Bad during one; same-key state dispatch, not two static actions. [bisect definitions](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-bisect.el#L57-L78) |

The previous draft's prose claimed reset `X` was a correction and called blame
unverifiable due to the absence of a blame transient. Both claims are
corrected here: reset-X is a stale top-level-prefix conflation, and blame's
entry commands plus its actual mode map provide evidence for all nine rows.

## Provenance by menu

The manual is the user-facing check; the same-release source is used to
disambiguate keys, state-conditional bindings, and source command identities.
Every URL is pinned to `659f89955cf60fe3d4326d881c412df06c69680d`.

| Local menu(s) | Manual | Same-release source |
| --- | --- | --- |
| commit | [Initiating a Commit](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L5767) | [magit-commit.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-commit.el#L117-L160) |
| branch | [Branching](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L6414) | [magit-branch.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-branch.el#L215-L267) |
| fetch / pull / push | [Fetching](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8485), [Pulling](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8579), [Pushing](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8644) | [magit-fetch.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-fetch.el#L34-L52), [magit-pull.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-pull.el#L42-L77), [magit-push.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-push.el#L42-L69) |
| merge | [Merging](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L7043) | [magit-merge.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-merge.el#L37-L65) |
| rebase | [Rebasing](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L7293) | [magit-sequence.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L524-L570) |
| stash | [Stashing](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8094) | [magit-stash.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-stash.el#L91-L118) |
| tag | [Tagging](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8937) | [magit-tag.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-tag.el#L37-L51) |
| reset | [Resetting](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8022) | [magit-reset.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-reset.el#L34-L46) |
| cherry-pick / revert | [Cherry Picking](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L7839), [Reverting](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L7976) | [magit-sequence.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L138-L166) and [revert](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L372-L392) |
| bisect | [Bisecting](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L4859) | [magit-bisect.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-bisect.el#L57-L78) |
| remote | [Remotes](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8296) | [magit-remote.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-remote.el#L66-L90) |
| submodule | [Submodules](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L9096) | [magit-submodule.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-submodule.el#L171-L192) |
| worktree | [Worktree](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L9312) | [magit-worktree.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-worktree.el#L128-L137) |
| ignore | [Ignoring](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L9162) | [magit-gitignore.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-gitignore.el#L34-L41) |
| log | [Logging](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L3203) | [magit-log.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-log.el#L525-L553) |
| diff | [Diffing](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L3747) | [magit-diff.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-diff.el#L1102-L1115) |
| patch-apply / patch-create | [Applying](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L5689) | [magit-sequence.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L429-L450), [magit-patch.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-patch.el#L37-L100) |
| blame | [Blaming](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L5168) | [magit-blame.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-blame.el#L946-L964) plus its read-only mode map |

## Verification record

The formerly completed roadmap checkbox was reopened because the draft table
was not consumed by tests. Completion evidence for this task is recorded in
`TASKS.md` only after the targeted menu test and the requested clean-environment
`make test` both pass. The static definition/data count is **192** (not the
draft's 194); the two-row overcount was corrected rather than preserving an
unsupported total. Verified `cargo test --test menus` (8 passed, including
the three injected-negative categories) and
`env -i HOME="$HOME" PATH="$PATH" make test` (Rust and 64 Neovim cases).
No production menu data is changed by this audit.
