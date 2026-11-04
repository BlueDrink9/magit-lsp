# LSP traces

`reconstructed-plain.jsonl` is reconstructed from the documented Plain-client
handoff examples. The user's raw Helix JSONL was unavailable; this is not an
actual Helix capture.

`plain-neovim-0.12.6.jsonl` contains four inbound wire messages extracted in
source order: the actual initialize request followed by the first three actual
`textDocument/didChange` notifications. `initialize-plain-neovim-0.12.6.json`
is the exact captured initialize `params`. Source: `/tmp/opencode/t002-retry/raw.jsonl`,
SHA256 `edd35eab32d2fa5a7ddb4bff599c518ee81407581ce6e35471ebcd5a58686370`.
Captured clientInfo is Neovim 0.12.6. Observed methods: `initialize`, then
three `textDocument/didChange`. Capture date: 2026-11-05. The known workflow
used the opt-in recorder with a plugin-free terminal Neovim session; exact
launch/cleanup commands are unavailable. Source log was recorder-redacted;
the temporary repository URI is retained as recorded, not replaced.

Coverage is partial: these notifications show full-document replacement,
insertion and deletion, but do not establish all six T034 edit shapes or
complete conformance. Other source messages are intentionally omitted.
