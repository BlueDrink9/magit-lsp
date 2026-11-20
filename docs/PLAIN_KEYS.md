# Plain key guidance

Authored guidance for the experimental, off-by-default Plain-key path. The
generated [Plain command support](./plain-support.md) reference lists which
commands each capability profile can run.

## Plain key policy

Plain keys are off by default. Policy resolves in order from the built-in default,
`plain.keys.default`, an exact `clientInfo.name` section, and a per-document toggle;
the data-driven client quirk limit takes precedence over all settings. The status
header reports the effective state and its source; the `Keys: on/off` code
action toggles only that document, and edits to the header are repaired. The
per-document toggle survives view-stack changes and is discarded when that
document closes. Prompt, log-selection and commit-message documents cannot be
armed. There is no repository-level setting.

## Plain-key safety limits

Plain-key recovery runs only commands that are implemented, non-destructive and
bound in the active profile; the server refuses anything else before touching
Git. Destructive commands (for example `status.discard`, `branch.delete`,
`tag.delete`, `reset.hard` and `commit.extend`) are therefore excluded from the
optional key path and stay reachable only through their explicit code actions;
typing their key reports that the code action is required. Existing
explicit-action prompt chains are unchanged: a dispatched key runs the same
command with its normal prompt or confirmation, and Plain-key recovery adds no
separate first-key confirmation and never auto-confirms or skips an existing
prompt. A multi-key paste, a planned command, a document that changed since it
was displayed, and keys that are off are refused without mutating Git. A
successful key edit is executed once and its typed text is repaired; the repair
echo does not replay the command. `tests/plain_keys_exec.rs` covers the eligible
single-key stage path and these refusals, including a `g r` chord and `-a`
commit-menu progression; `tests/ui/scenarios/plain-keys-recovered.scn` covers
the off-by-default and per-document toggle flow. Hunk, visual, unstage and
navigation cases and the wider Phase 9 matrix (T034–T039) are not covered here
and remain unverified.
