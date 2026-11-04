# LSP traces

`reconstructed-plain.jsonl` is reconstructed from the documented Plain-client
handoff examples. The original raw Helix source survives at
`/tmp/opencode/helix-capture-1793822958/raw-parent/raw.jsonl` with SHA256
`66b2b40fc281625336cbaf808fc0455fe324f8e8110b740d310433141f8d9eda`.

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

Reproduce from any checkout subdirectory on Linux with Helix 25.07.1, tmux and
`jq`. This uses the exact captured Helix binary. Fresh runs have different PID,
timestamp and temporary URI; compare each extraction structurally to its own
source. The original source path and hash above identify the old capture only.

```sh
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
cargo build --manifest-path "$ROOT/Cargo.toml"
D=$(mktemp -d /tmp/opencode/helix-capture-XXXXXXXX)
SOCKET="helixcap-$$"
SESSION="cap-$$"
mkdir -p "$D/home" "$D/config/helix" "$D/repo" "$D/raw-parent"
cleanup() { tmux -L "$SOCKET" kill-server 2>/dev/null || true; rm -rf "$D"; }
trap 'status=$?; if test "$status" -ne 0; then echo "Helix capture retained at: $D" >&2; tmux -L "$SOCKET" capture-pane -p -t "$SESSION":0.0 2>/dev/null >&2 || true; fi' EXIT
git -C "$D/repo" init -q
SERVER=$(realpath "$ROOT/target/debug/magit-lsp")
cat > "$D/config/helix/languages.toml" <<EOF
[language-server.magit-lsp]
command = "$SERVER"

[[language]]
name = "magit"
scope = "source.magit"
file-types = [{ glob = "*.magit" }]
roots = [".git"]
language-servers = ["magit-lsp"]
EOF

HX=/nix/store/1nc2y094mdd6z9c4bgm766jwmxsf3mzd-helix-25.07.1/bin/hx
test "$("$HX" --version | head -1)" = 'helix 25.07.1'
printf 'hello\n' > "$D/repo/sample.magit"
tmux -L "$SOCKET" new-session -d -s "$SESSION" "cd '$D/repo' && HOME='$D/home' XDG_CONFIG_HOME='$D/config' MAGIT_LSP_WIRELOG='$D/raw-parent/raw.jsonl' '$HX' sample.magit"
HX_PID=$(tmux -L "$SOCKET" display-message -p -t "$SESSION" '#{pane_pid}')
export SOURCE="$D/raw-parent/raw.jsonl"
python3 - <<'PY'
import json, os, time
deadline = time.monotonic() + 30
while time.monotonic() < deadline:
    try:
        with open(os.environ['SOURCE']) as f:
            if any(json.loads(line).get('raw', {}).get('method') == 'initialize' for line in f):
                break
    except (FileNotFoundError, json.JSONDecodeError):
        pass
else:
    raise SystemExit('timed out waiting for initialize')
PY
jq -e 'select(.raw.method == "initialize")' "$D/raw-parent/raw.jsonl" >/dev/null
export SOCKET SESSION
python3 - <<'PY'
import json, os, subprocess, time
source = os.environ['SOURCE']
socket, session = os.environ['SOCKET'], os.environ['SESSION']
def read_rows():
    with open(source) as f:
        rows = []
        for line in f:
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                continue
        return rows
deadline = time.monotonic() + 30
while time.monotonic() < deadline:
    rows = read_rows()
    if any(row.get('raw', {}).get('method') == 'textDocument/didOpen' for row in rows):
        break
else:
    raise SystemExit('timed out waiting for Helix didOpen')
def changes():
    rows = read_rows()
    pid = next(row['pid'] for row in rows if row.get('raw', {}).get('method') == 'initialize')
    return sum(row.get('pid') == pid and row.get('raw', {}).get('method') == 'textDocument/didChange' for row in rows)
def wait_changes(expected):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if changes() >= expected:
            return
    raise SystemExit(f'timed out waiting for {expected} Helix didChange rows; saw {changes()}')
actions = [(['i', 'X', 'Escape'], 1), (['o', 'Escape'], 2), (['o', 'Escape'], 3)]
for keys, expected in actions:
    subprocess.run(['tmux', '-L', socket, 'send-keys', '-t', f'{session}:0.0', *keys], check=True)
    wait_changes(expected)
subprocess.run(['tmux', '-L', socket, 'send-keys', '-t', f'{session}:0.0', 'Escape'], check=True)
screen = subprocess.check_output(['tmux', '-L', socket, 'capture-pane', '-p', '-t', f'{session}:0.0'], text=True)
print(screen)
if 'INSERT' in screen.upper():
    raise SystemExit('Helix still reports insert mode; refusing quit command')
subprocess.run(['tmux', '-L', socket, 'send-keys', '-t', f'{session}:0.0', ':quit-all!', 'Enter'], check=True)
PY
export SOCKET SESSION
python3 - <<'PY'
import json, os, subprocess, time
socket, session = os.environ['SOCKET'], os.environ['SESSION']
deadline = time.monotonic() + 30
while time.monotonic() < deadline:
    if subprocess.run(['tmux', '-L', socket, 'has-session', '-t', session],
                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        break
else:
    raise SystemExit('timed out waiting for graceful Helix shutdown')
source = os.environ['SOURCE']
deadline = time.monotonic() + 15
while time.monotonic() < deadline:
    with open(source) as f:
        rows = []
        for line in f:
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    init = next(row for row in rows if row.get('raw', {}).get('method') == 'initialize')
    pid = init['pid']
    if any(row.get('pid') == pid and row.get('raw', {}).get('method') in ('shutdown', 'exit') for row in rows) and not __import__('os').path.exists(f'/proc/{pid}'):
        break
else:
    raise SystemExit('timed out waiting for recorder shutdown/exit rows and server PID drain')
PY
test -s "$D/raw-parent/raw.jsonl"
sha256sum "$D/raw-parent/raw.jsonl"
PID=$(jq -r 'select(.raw.method == "initialize") | .pid' "$D/raw-parent/raw.jsonl" | head -1)
jq -c "select(.direction == \"in\" and .pid == $PID and (.raw.method == \"initialize\" or .raw.method == \"textDocument/didChange\"))" "$D/raw-parent/raw.jsonl" > "$D/generated.jsonl"
jq -c "select(.direction == \"in\" and .pid == $PID and .raw.method == \"initialize\") | .raw.params" "$D/raw-parent/raw.jsonl" > "$D/initialize.json"
test "$(wc -l < "$D/generated.jsonl")" -eq 4
cmp -s "$D/generated.jsonl" <(jq -c "select(.direction == \"in\" and .pid == $PID and (.raw.method == \"initialize\" or .raw.method == \"textDocument/didChange\"))" "$D/raw-parent/raw.jsonl")
cmp -s "$D/initialize.json" <(jq -c "select(.direction == \"in\" and .pid == $PID and .raw.method == \"initialize\") | .raw.params" "$D/raw-parent/raw.jsonl")

# Validate original committed evidence against its source when source is retained.
ORIGINAL=/tmp/opencode/helix-capture-1793822958/raw-parent/raw.jsonl
if test -f "$ORIGINAL"; then
  sha256sum "$ORIGINAL"
  OLD_PID=$(jq -r 'select(.raw.method == "initialize") | .pid' "$ORIGINAL" | head -1)
  jq -S -c "select(.direction == \"in\" and .pid == $OLD_PID and (.raw.method == \"initialize\" or .raw.method == \"textDocument/didChange\"))" "$ORIGINAL" | cmp - <(jq -S -c . "$ROOT/tests/fixtures/lsp_traces/helix-25.07.1.jsonl")
  jq -S "select(.direction == \"in\" and .pid == $OLD_PID and .raw.method == \"initialize\") | .raw.params" "$ORIGINAL" | cmp - <(jq -S . "$ROOT/tests/fixtures/lsp_traces/initialize-helix-25.07.1.json")
else
  echo "Original source unavailable; cannot compare committed fixtures: $ORIGINAL" >&2
fi
cleanup
```

Fresh-run outputs are checked against their own source. If the retained original
source is available, the recipe also compares its structurally extracted rows
and initialize params against committed fixtures. Cleanup runs only after all
checks pass; otherwise inspect the retained directory printed by `mktemp`.

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
