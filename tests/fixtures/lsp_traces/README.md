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

### Native Helix 25.07.1 capture

`helix-25.07.1.jsonl` retains actual initialize and three native
`textDocument/didChange` notifications from one recorder PID, in source order.
`initialize-helix-25.07.1.json` stores exact initialize params. Captured
2026-11-05 with `/nix/store/1nc2y094mdd6z9c4bgm766jwmxsf3mzd-helix-25.07.1/bin/hx`
(`hx --version`: 25.07.1). Dated release check:
`https://api.github.com/repos/helix-editor/helix/releases/latest` reported
tag 25.07.1 on 2026-11-05. Recorder source SHA256:
`66b2b40fc281625336cbaf808fc0455fe324f8e8110b740d310433141f8d9eda`.

Reproduce from repository root after `cargo build`; set `D` to a new
`/tmp/opencode/helix-capture-*`. Create `$D/home`, `$D/config/helix`,
`$D/repo/.git` and `$D/raw-parent` (recorder parent must exist). Put this in
`$D/config/helix/languages.toml`:

```toml
[language-server.magit-lsp]
command = "/home/william/src/magit-lsp/target/debug/magit-lsp"

[[language]]
name = "magit"
scope = "source.magit"
file-types = [{ glob = "*.magit" }]
roots = [".git"]
language-servers = ["magit-lsp"]
```

Then run:

```sh
printf 'hello\n' > "$D/repo/sample.magit"
HX=/nix/store/1nc2y094mdd6z9c4bgm766jwmxsf3mzd-helix-25.07.1/bin/hx
tmux -L helixcap new-session -d -s cap2 "cd '$D/repo' && HOME='$D/home' XDG_CONFIG_HOME='$D/config' MAGIT_LSP_WIRELOG='$D/raw-parent/raw.jsonl' '$HX' sample.magit"
tmux -L helixcap send-keys -t cap2:0.0 i X Escape
tmux -L helixcap send-keys -t cap2:0.0 x p a Enter
tmux -L helixcap capture-pane -p -t cap2:0.0
tmux -L helixcap kill-server
rm -rf "$D"
```

The capture's retained rows 2–4 show `X` inserted at 0:0, then newline
insertions at 1:0. None replaces a nonempty range. This ordinary attached file
is not a server-owned status view. Do not infer input action from edit text.
`reconstructed-plain.jsonl` remains historical reconstruction.

### T034 action evidence matrix

“Observed” means retained payload proves that change shape; it does not prove
which user action caused it. Actions count as captured only with native action
provenance; unavailable actions remain explicit.

| Native action | Helix 25.07.1 | Neovim traces |
|---|---|---|
| Single insert | Observed payload shape (Helix row 2); native single-key cause documented. | Native A insertion in plain trace row 3; retained. |
| Selection replace | NOT CAPTURED; no nonempty replacement range in Helix rows 2–4. | NOT CAPTURED; no native selection-replacement action evidence. |
| Paste one character | NOT CAPTURED. | NOT CAPTURED. |
| Paste many characters | NOT CAPTURED. | NOT CAPTURED. |
| Rapid typing | NOT CAPTURED; separate rapid input not tested. | NOT CAPTURED. |
| Undo immediately after server repair edit | NOT CAPTURED. | NOT CAPTURED. |

Plain Neovim row 2 is a server-rendered insertion, row 3 native `A`
insertion, and row 4 `A` deletion/repair; these do not establish the missing
actions. Bundled Neovim's commit-draft change is a native full-buffer
replacement, not proof of any particular requested input action. Bundled
VSCode's retained change is also a full-buffer replacement; no native action
among the six is established. Thus all six actions remain incomplete for at
least one or both requested editor families.
