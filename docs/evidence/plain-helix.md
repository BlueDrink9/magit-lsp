# Plain Helix manual smoke

Linux, 2026-11-16; Helix **25.07.1**. This is a manual checklist, not a
Helix test suite or a full support claim. No editor-specific adapter was loaded.
The generic TOML in [the client spec](../CLIENT_SPEC.md#generic-helix-reference-configuration)
was used unchanged by `make helix-smoke` (set `HELIX_BIN` to the installed `hx`).

## Results

| Check | Observed result |
| --- | --- |
| Open | Pass: opening the provider file `.magit` attached the server. |
| Status | Pass: server-rendered status showed both modified files. |
| Stage | Pass: native code action staged `f.txt`; Git's cached diff contained exactly that file (two insertions and two deletions). |
| Menu | Pass: native code actions opened the commit menu. |
| Log | Pass: log menu's Open current action opened the concrete graph log. |
| Commit | Fail: generic configuration does not attach the server to `COMMIT_EDITMSG`. Manual `:set-language magit` reused the connection, but confirming an already-open draft did not prevent the unopened-draft timeout. No commit was created. |
| Abort | Blocked after the timeout: commit actions became disabled with “the commit draft was not opened in time; retry the commit”, despite the instruction to retry. Native Abort completion is not claimed. |

Helix omitted `window.showDocument` in initialize; the effective server
capability was false. The server correctly delivered readable manual open and
Confirm/Abort instructions instead. There was one initialize exchange, including
after manual language attachment.

## Retained local evidence

`build/ui/helix-t021/` contains `status.screen`, `staged.screen`, `menu.screen`,
`log.screen`, `commit-instructions.screen`, `commit-unattached.screen`, the native
`helix.log`, and session/cleanup JSON. `build/ui/helix-t021-next/` contains the
retry log and `commit-actions.screen`. These are local text/structured evidence,
not committed fixtures.

In the retry log, the commit began at 02:34:44, Confirm was offered at 02:34:51
and executed at 02:35:01; nevertheless the unopened-draft timeout fired at
02:35:14. The buffer had attached before this run, so no new didOpen occurred.
Git HEAD remained `f469302e1771a7bb207608f28b7decd4169cc6fc` (`initial`).
This establishes the failed native path, not a complete diagnosis.

Both sessions were stopped through `make helix-smoke-stop`; cleanup records
distinguish non-executing servers from retained defunct `/proc` entries.
No fully reaped-process or non-Linux verification claim is made.

Follow-ups are tracked as **R-T021-ATTACH** and **R-T021-DRAFT** in the
[authoritative queue](../../TASKS.md#review). Abort remains part of the latter's
acceptance, rather than being silently substituted by timeout cancellation.

## Follow-up verification after lifecycle fixes

On the same Linux/Helix version and date, `build/ui/helix-t021-fixed/helix.log`
records successful manual Confirm and Abort. The generic TOML was unchanged;
`:set-language magit` attached the first draft promptly. Confirm created
`e928ce0ca5a846378110156f481e70f37f6c342a` with message
`Helix native smoke commit`; Git status then contained only unstaged `g.txt`.
The next run reused the open draft, received its new template, and offered Abort.
`abort-before.json` and `abort-after.json` match exactly for HEAD, index tree
`2d605f6ee74dbaebd2c4abfcffd8b563ef4f6c36` and SHA-256 of both worktree files.
Cleanup through the Make stop target succeeded. Thus Confirm/Abort now pass with
explicit manual first attachment; the earlier failures above remain provenance.
