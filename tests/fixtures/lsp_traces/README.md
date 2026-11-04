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

`bundled-neovim-0.12.6.jsonl` contains the actual initialize request and the
native commit-draft `textDocument/didChange` (source row 66), in source order.
It omits the earlier status-view echo (source row 12). The initialize params
are also stored in `initialize-bundled-neovim-0.12.6.json`. Source:
`/tmp/opencode/t002-bundled/raw.jsonl`, SHA256
`f0a8cae59cd5e6a0605e63330c97baa14a12176c4b5807dc96d12f0dca63b612`.
The source recorded bundled Neovim client version 0.12.6. Observed draft:
real `.git/COMMIT_EDITMSG`; native insertion changed its text to
`Bundled native capture`. The draft snapshot showed version 3 and modified
state; the message was automatically recorded as `didChange`.

Reproduce from repository root (Neovim and UI test prerequisites installed):

```sh
mkdir -p /tmp/opencode/t002-bundled
PYTHONPATH=scripts python3 - <<'PY'
from ui_lib import driver
from pathlib import Path
run = driver.start('staged-one-file', extra_env={
    'MAGIT_LSP_WIRELOG': '/tmp/opencode/t002-bundled/raw.jsonl',
    'MAGIT_LSP_BIN': str(Path('target/debug/magit-lsp').resolve()),
})
try:
    print(driver.snapshot(run))  # status view
    driver.send_keys(run, [':vsplit', 'Enter'])
    driver.send_keys(run, ['c'])
    driver.wait_idle(run)
    driver.send_keys(run, ['c'])
    driver.wait_idle(run)  # real commit draft
    print(driver.capture(run))
    driver.send_keys(run, ['g', 'g', 'd', 'G', 'i', 'Bundled native capture'])
    driver.wait_idle(run)
    print(driver.capture(run))
    print(driver.snapshot(run))  # real draft, modified, version 3
finally:
    driver.stop(run)
PY
```

The capture is opt-in and recorder-redacted. Only initialize and the native
commit-draft didChange are retained; remaining raw messages are omitted. The
temporary source log is owned by this run and may be removed with
`rm -f /tmp/opencode/t002-bundled/raw.jsonl`.

`bundled-vscode-1.140.06880.jsonl` contains PID 1417132's actual initialize
request and first native `textDocument/didChange`, in source order;
`initialize-bundled-vscode-1.140.06880.json` stores the exact initialize
`params`. Source: `/tmp/opencode/t002-vscode-fixed/raw.jsonl`, SHA256
`556f8246c5c5cc1b4e40dfd9499cbc26ed0183b283f42cdd0e13c80525841b42`.
Observed clientInfo is VSCodium 1.140.0; the CLI host build is 1.140.06880
and the shipped extension is 0.1.0. The actual wire initialize omits
`experimental.magit`, despite the extension source intending that declaration.
Treat this observed mismatch as evidence for T004's capability audit; no
declaration is fabricated. The isolated normal-host launcher loads the
production extension; initialize options report profile `standard`.

The native evidence at
`/tmp/opencode/vscode-review-8AR4IX/real-file-native-evidence.json` reports
`status: pass` for production real-file native close. The source contains 298
valid rows; this extraction keeps initialize and the first change from the
same PID. Other messages and the synthetic semantic-owner probe from PID
1418535 are excluded. The recorded run exited 0 and its launcher terminates
only its owned editor processes.

Reproduce from repository root (VSCodium and display available):

```sh
mkdir -p /tmp/opencode/t002-vscode-fixed
MAGIT_VSCODE_BIN=$(command -v codium) MAGIT_VSCODE_EDIT_ONLY=1 MAGIT_LSP_WIRELOG=/tmp/opencode/t002-vscode-fixed/raw.jsonl npm --prefix clients/vscode run test:e2e
```
