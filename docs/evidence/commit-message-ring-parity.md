# Commit-message ring source comparison

Source checked 2026-10-29 against Magit commit
[`4031049049b23785268497ff49eed0690f1ce85f`](https://github.com/magit/magit/commit/4031049049b23785268497ff49eed0690f1ce85f),
`lisp/git-commit.el`. Relevant pinned symbols: setup installs
`with-editor-pre-finish-hook` and `with-editor-pre-cancel-hook` at lines
578–579; both call `git-commit-save-message`. `git-commit-save-message`
(814–825) obtains `git-commit-buffer-message`, removes any existing equal
entry via `ring-member`/`ring-remove`, and inserts it into
`log-edit-comment-ring`. It rejects messages for which that function returns
nil with “Only whitespace and/or comments; message not saved.” Therefore both
successful finish and abort retain the current *effective* message before the
terminal result, not merely a nonempty raw buffer. `git-commit-buffer-message`
(832–852) removes comment lines and the verbose-diff tail, rejects
whitespace-only content, and trims repeated leading/trailing newlines to one.
The default ring is Emacs's `log-edit-comment-ring`; configurable
`git-commit-use-local-message-ring` (253–260, 828–831) selects repository-local
storage instead. Ring capacity comes from `log-edit-maximum-comment-ring-size`,
not a Magit hard-coded count/byte cap. Equal messages are de-duplicated
wherever found in the ring, then inserted newest; this is not adjacent-only
exact-text de-duplication. The documented `M-p`/`M-n` behavior (66–77) stores
the current buffer before navigating history.

Project comparison: `src/editor/history.rs` retains the terminal saved-file
text supplied by the finish/abort paths (`src/editor.rs`), with no Git-log
source. It excludes raw empty and text exactly equal to the initial template,
retains other raw text (including whitespace/comment-only text), compares
exact bytes, and only suppresses an exact *adjacent* duplicate. It is shared
server-lifetime state keyed by canonical repository, capped at 50 entries and
1 MiB aggregate UTF-8 bytes; a single over-limit submission is not retained.
Private unit tests cover those eligibility, dedup, eviction and byte-limit
rules, and repository/registry isolation. Unlike Magit, project retention uses
the saved file, not didSave or unsaved synchronized text.

This is a documented behavioral mismatch, not a claim of full Magit parity.
Whether to normalize/strip Git comments and verbose diff, reject effective
empty messages, de-duplicate globally, or alter scope/caps requires an explicit
product-policy decision. Do not infer Git's effective-empty rules from raw
eligibility: Git owns cleanup and may reject/transform a message independently.
The source comparison does not establish behavior for every Git invocation,
Emacs configuration, or local-ring persistence backend.
