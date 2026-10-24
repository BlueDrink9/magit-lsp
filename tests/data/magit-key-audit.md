# Magit menu-key audit draft

## Pin and method

Reference is Magit **v4.7.1**, released 2026-09-01. GitHub's release API
reports that release; the annotated tag `v4.7.1` resolves to commit
`659f89955cf60fe3d4326d881c412df06c69680d` (tag object
`bb79175d7c2ddc309b89f33b499c8f27d7965503`). All source links below use the
immutable commit, not `master` or the mutable release page. The pinned manual
is the `docs/magit.texi` from that commit. The transient definitions are the
Magit source at the same commit (Magit requires Transient >= 0.13; Transient
is a separately versioned dependency, not a Magit release artifact).

This pass transcribed suffix keys into `magit-keys.txt` from each static menu
definition and compared them with the pinned manual and the corresponding
same-release transient/keymap. A suffix is the key *after* the top-level
prefix. Manual `@kindex` entries were used as navigation only: a command can
be listed at another prefix and also occur in a transient. In-progress states,
conditional/hidden suffixes, and contextual aliases were checked against the
transient definition rather than rejected based on the first manual index.

## Coverage

* 22 static menu definitions; **194 action occurrences** transcribed, including
  duplicate commands appearing in distinct menus. The source-of-record rows
  are grouped by menu in `magit-keys.txt`; this is a review draft and is not
  consumed by tests yet.
* Coverage excludes switches/options, the generated dispatch menu, the common
  `?` opener inserted by `menus::build`, profile-dependent dispatch mappings,
  and behavior selected dynamically by repository/in-progress state. These
  are not static menu actions and must not be added as fake reference rows.
* 185 menu/key occurrences are supported by same-release menu/manual evidence
  (including conditional actions and same-key state aliases). The 9 `blame`
  rows are marked unsupported/unverifiable as a single static menu inventory:
  Magit has no `magit-blame` transient corresponding to this local menu; its
  manual documents blame entry/command bindings and blame-buffer operations
  separately. Resolve those nine from the pinned blame mode map/manual before
  treating them as normal transient parity.
* No confirmed key mismatch is established by this bounded pass. In
  particular, the old claimed reset-X discrepancy is not a v4.7.1 discrepancy.

## Recorded exceptions / historical claims

| Claim from old audit | v4.7.1 evidence and treatment |
| --- | --- |
| reset `X` | **Not an exception.** The reset transient binds `b`, `f`, `m`, `s`, `h`, `k`, `i`, `w`; no `X` action exists. Local reset uses those keys. Drop the old claim rather than white-listing a phantom key. |
| rebase `s` / `e` | Both are actual default actions: `s` = subset and `e` = elsewhere/branch. During an in-progress rebase, `s` means Skip and `e` means Edit. These are conditional aliases in one transient, not extra static actions or mismatches; retain as a narrowly documented **state-alias rule** if the test checks command identity. |
| bisect `B` | `B` means Start before a bisect and Bad during a bisect. This is a same-key conditional alias in the transient. Keep one static key row; do not require separate rows per state. |

The previous parity table's prose is therefore not authoritative. The `X`
statement is contradicted by this release; rebase and bisect describe
state-dependent reuse, not key corrections.

## Provenance by menu

Each source URL is pinned to commit `659f89955cf60fe3d4326d881c412df06c69680d`.
Manual anchors identify the corresponding section; source ranges identify the
transient definitions (some entries are conditional in those ranges).

| Local menu | Magit v4.7.1 manual section | Same-release source |
| --- | --- | --- |
| commit | [Initiating a Commit](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L5767) | [magit-commit.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-commit.el#L117-L160) |
| branch | [Branching](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L6414) | [magit-branch.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-branch.el#L215-L267) |
| fetch / pull / push | [Fetching](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8485), [Pulling](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8579), [Pushing](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8644) | [magit-fetch.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-fetch.el#L34-L52), [magit-pull.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-pull.el#L42-L77), [magit-push.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-push.el#L42-L69) |
| merge | [Merging](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L7043) | [magit-merge.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-merge.el#L37-L65) |
| rebase | [Rebasing](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L7293) | [magit-sequence.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L524-L570) |
| stash | [Stashing](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8094) | [magit-stash.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-stash.el#L91-L118) |
| tag | [Tagging](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8937) | [magit-tag.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-tag.el#L37-L51) |
| reset | [Resetting](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8022) | [magit-reset.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-reset.el#L34-L46) |
| cherry-pick / revert | [Cherry Picking](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L7839), [Reverting](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L7976) | [magit-sequence.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L138-L166) and [revert definition](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L372-L392) |
| bisect | [Bisecting](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L4859) | [magit-bisect.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-bisect.el#L57-L78) |
| remote | [Remotes](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L8296) | [magit-remote.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-remote.el#L66-L90) |
| submodule | [Submodules](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L9096) | [magit-submodule.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-submodule.el#L171-L192) |
| worktree | [Worktree](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L9312) | [magit-worktree.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-worktree.el#L128-L137) |
| ignore | [Ignoring](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L9162) | [magit-gitignore.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-gitignore.el#L34-L41) |
| log | [Logging](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L3203) | [magit-log.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-log.el#L525-L553) |
| diff | [Diffing](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L3747) | [magit-diff.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-diff.el#L1102-L1115) |
| patch-apply / patch-create | [Applying](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L5689) | [magit-sequence.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-sequence.el#L429-L450), [magit-patch.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-patch.el#L37-L100) |
| blame | [Blaming](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/docs/magit.texi#L5168) | [magit-blame.el](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-blame.el#L946-L964); [mode keymap](https://github.com/magit/magit/blob/659f89955cf60fe3d4326d881c412df06c69680d/lisp/magit-blame.el) |

## Follow-up implementation

Minimal production/test targets for the next ticket: `tests/data/magit-keys.txt`,
`tests/data/magit-key-audit.md`, and `tests/menus.rs`. Parse the data as
independent tuples; compare every action in every static menu against the data
and every data tuple against exactly one action. Assert duplicate command keys
are allowed only when their menu differs. Keep a narrow `EXCEPTIONS` table
keyed by `(menu, key, state)` for documented conditional aliases only; assert
each exception names an observed action and contains its reason. Do not include
reset-X (not in v4.7.1). For blame, either finish evidence from the pinned
manual and mode map and include its nine entries, or explicitly keep that
menu outside the transient parity assertion with a cited reason; never claim
those keys came from a nonexistent transient.

This draft has not been wired into or validated by the existing parity tests.
