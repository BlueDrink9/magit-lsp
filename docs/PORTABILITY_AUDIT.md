# Documentation path portability audit

Scope: documentation text under version control, with focused edits to the
three historical evidence reports, `TASKS_PART_SEVEN.md`, and this record.
Search outcome: the bounded tracked-text sweep found the personal checkout
prefix only in the two audit statements claiming it absent; both statements
are corrected here and in PORT2 below.
The four historical reports have normalization notices; their commands are
not reruns and do not alter the original results.

## Decisions

- **Remove:** personal absolute checkout and executable prefixes. Repository
  commands use checkout-relative paths; Codium uses `command -v codium`.
- **Portabilize:** archived formatter commands use the caller-supplied
  `$MAGIT_LSP_ROOT/Makefile`, preserving separate `/tmp/opencode` fixtures.
- **Retain:** `/tmp/opencode` evidence directories and logs identify retained
  artifacts and provenance, not active repository dependencies.
- **Retain:** synthetic `/home/alice`, generic `/home/you` and
  `/home/YOUR_USER`, and redacted `/home/<user>` examples; these demonstrate
  fixtures, placeholders or historical redaction, not personal checkouts.
- **Retain:** dated historical snapshots, preserving their dated provenance.
- **Retain:** the maintained `/usr/bin/git` test dependency, owned by
  **Resolve the real Git test binary through PATH** in the queue.
- **Retain:** `/tmp` paths for explicit temporary fixtures, wire URIs and
  historical artifact provenance; these are not personal home paths.

## Reviewer counter-assessment

@reviewer: dated provenance and the lifecycle spike should remain; synthetic
fixtures are not host leaks. The PATH-based runner fix and brittle checker
retirement are complete; `tmp-task.md` was deliberately deleted. This audit
agrees: it normalizes actual host prefixes only, keeps artifact provenance,
and does not rerun or broaden the evidence. Review counter-assessment recorded;
  no additional path-dependent documentation change identified by the bounded
  tracked-text sweep.
