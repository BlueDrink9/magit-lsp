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

#### Helix selection and register-paste evidence

`helix-25.07.1-actions.jsonl` contains the original source rows 10, 16 and 22
verbatim (not reconstructed from the fresh run). Its sidecar records exact
native key sequences, source row numbers, source SHA256
`ed10a9d46467228bf8250cd44064aa94466c5fea6b03e0ed25d7dbb9e788c078`, the
repair/echo/reply rows, and the original cleanup limitation. The three actual
edits were `Hea`→`XXX` over range 0:0–0:3, insertion of `H` at 0:1, and
insertion of `Hea` at 0:1. Each action was followed by server repair, didChange
echo and successful applyEdit reply. Saved native text before and after each
action is identical: server repair restored the baseline, as intended. This
does not negate the action; the source didChange rows establish it. Yanks used
to fill registers `a` and `b` were setup, not edits.

The original capture later acquired unrelated cleanup keypress edits in rows
27–39 and ended without graceful shutdown. Those tail rows are excluded from
the selected action evidence; the three selected rows remain verbatim and the
full original source hash is stable. The corrected driver now checks the
Helix pane is out of insert/select mode and sends `:quit-all!` directly, without
a preceding Escape. Fresh validation was run from the repository root with:

```sh
ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
cargo build --manifest-path "$ROOT/Cargo.toml"
nix shell nixpkgs#helix --command python3 - <<'PY'
import hashlib, json, os, pathlib, shutil, subprocess, tempfile, time

ROOT=pathlib.Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip()).resolve()
HX=shutil.which('hx')
if not HX: raise SystemExit('hx missing inside nix shell nixpkgs#helix')
TAG=f'{os.getpid()}-{time.time_ns()}'; D=pathlib.Path(tempfile.mkdtemp(prefix=f't002-helix-{TAG}-',dir='/tmp/opencode'))
REPO,HOME,CONFIG=D/'repo',D/'home',D/'config'; RAW=D/'raw-parent/raw.jsonl'
SOCKET=SESSION=f't002hx-{TAG}'; TMUX=['tmux','-L',SOCKET]; SERVER=(ROOT/'target/debug/magit-lsp').resolve()
def cmd(args,**kw): return subprocess.run(args,check=True,text=True,**kw)
def rows(): return [json.loads(x) for x in RAW.read_text().splitlines() if x.strip()] if RAW.exists() else []
def wait(pred,label,timeout=20):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  if (value:=pred()): return value
  time.sleep(.05)
 raise RuntimeError('timeout waiting for '+label)
def screen(): return subprocess.check_output(TMUX+['capture-pane','-p','-t',f'{SESSION}:0.0'],text=True)
def keys(*ks): cmd(TMUX+['send-keys','-t',f'{SESSION}:0.0',*ks])
def changes():
 rr=rows(); init=next((r for r in rr if r.get('raw',{}).get('method')=='initialize'),None)
 return [r for r in rr if init and r.get('pid')==init.get('pid') and r.get('raw',{}).get('method')=='textDocument/didChange']
def text():
 path=REPO/'.magit'
 try: stamp=(path.stat().st_ino,path.stat().st_mtime_ns,path.stat().st_size)
 except FileNotFoundError: stamp=None
 keys(':write','Enter')
 wait(lambda:path.exists() and (path.stat().st_ino,path.stat().st_mtime_ns,path.stat().st_size)!=stamp,'native save')
 return path.read_text()
report=None; REPO.mkdir(); HOME.mkdir(); (CONFIG/'helix').mkdir(parents=True); RAW.parent.mkdir()
try:
 if subprocess.check_output([HX,'--version'],text=True).strip()!='helix 25.07.1': raise RuntimeError('wrong Helix version')
 cmd(['git','init','-q',str(REPO)]); cmd(['git','config','user.name','Capture'],cwd=REPO); cmd(['git','config','user.email','capture@example.invalid'],cwd=REPO)
 (REPO/'tracked').write_text('base\n'); cmd(['git','add','tracked'],cwd=REPO); cmd(['git','commit','-qm','base'],cwd=REPO); (REPO/'.magit').write_text('Loading…\n')
 if not SERVER.exists(): cmd(['cargo','build','--manifest-path',str(ROOT/'Cargo.toml')],cwd=ROOT)
 (CONFIG/'helix/languages.toml').write_text(f'''[language-server.magit-lsp]\ncommand = "{SERVER}"\n\n[[language]]\nname = "magit"\nscope = "source.magit"\nfile-types = [{{ glob = ".magit" }}]\nroots = [".git"]\nlanguage-servers = ["magit-lsp"]\n''')
 env=os.environ.copy(); env.update(HOME=str(HOME),XDG_CONFIG_HOME=str(CONFIG),XDG_DATA_HOME=str(D/'data'),XDG_STATE_HOME=str(D/'state'),MAGIT_LSP_WIRELOG=str(RAW),GIT_CONFIG_NOSYSTEM='1',GIT_TERMINAL_PROMPT='0')
 cmd(['tmux','-L',SOCKET,'new-session','-d','-s',SESSION,'-x','120','-y','40',f"cd '{REPO}' && HOME='{HOME}' XDG_CONFIG_HOME='{CONFIG}' MAGIT_LSP_WIRELOG='{RAW}' '{HX}' .magit"],env=env)
 wait(lambda:any(r.get('raw',{}).get('method')=='textDocument/didOpen' for r in rows()),'didOpen'); wait(lambda:'Head:' in screen() and 'Loading' not in screen(),'render')
 req=next((r for r in rows() if r.get('direction')=='out' and r.get('raw',{}).get('method')=='workspace/applyEdit'),None)
 if not req: raise RuntimeError('missing initial applyEdit')
 wait(lambda:any(r.get('direction')=='in' and r.get('raw',{}).get('id')==req['raw'].get('id') for r in rows()),'render reply')
 report={'directory':str(D),'helix_version':subprocess.check_output([HX,'--version'],text=True).strip(),'source':str(RAW),'actions':[]}
 actions=[('selection_replace',['Escape','g','g','g','h','v','l','l','r','X']),('named_register_paste_one',['Escape','g','g','g','h','"','a','y','Escape','g','g','g','h','"','a','p']),('named_register_paste_multi',['Escape','g','g','g','h','v','l','l','"','b','y','Escape','g','g','g','h','"','b','p'])]
 for name,sequence in actions:
  before=text(); count=len(changes()); keys(*sequence); wait(lambda:changes() if len(changes())>count else None,name+' didChange'); after=text()
  report['actions'].append({'name':name,'keys':sequence,'before':before,'after':after})
finally:
 if subprocess.run(TMUX+['has-session','-t',SESSION],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0:
  pane=screen()
  if 'INSERT' in pane.upper() or 'SELECT' in pane.upper(): raise RuntimeError('not in normal mode; refusing quit')
  cmd(TMUX+['send-keys','-t',f'{SESSION}:0.0',':quit-all!','Enter'])
  wait(lambda:subprocess.run(TMUX+['has-session','-t',SESSION],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0,'graceful shutdown',8)
 subprocess.run(['tmux','-L',SOCKET,'kill-server'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
if report is None: raise RuntimeError(f'capture failed; retained at {D}')
deadline=time.monotonic()+15
while time.monotonic()<deadline:
 rr=rows(); init=next((r for r in rr if r.get('raw',{}).get('method')=='initialize'),None); pid=init.get('pid') if init else None
 if pid and any(r.get('pid')==pid and r.get('raw',{}).get('method')=='exit' for r in rr):
  try: os.kill(pid,0)
  except ProcessLookupError: break
 time.sleep(.05)
else: raise RuntimeError(f'recorder did not drain; retained at {D}')
report['source_sha256']=hashlib.sha256(RAW.read_bytes()).hexdigest(); report['source_rows']=len(rows())
print(json.dumps(report,indent=2)); print(f"drained source {RAW}; SHA256 {report['source_sha256']}; rows {report['source_rows']}; PID {pid} exited")
shutil.rmtree(D)
PY
```

This embedded recipe resolves the checkout via Git, obtains Helix through Nix,
uses owned temporary resources, waits for the actual render, performs native
save-based before/after inspection and drains shutdown/exit before printing
source hash and row count. Fresh runs validate the procedure, not historical
PID/time/hash. Original capture cleanup limitation remains: its tail rows 27–39
are excluded and it has no graceful shutdown; its three action rows remain
source-equal.

Executed from `tests/fixtures` on 2026-11-05: exit 0; fresh source
`/tmp/opencode/t002-helix-3209274-1793834905552565670-hrwjzczo/raw-parent/raw.jsonl`,
SHA256 `b9aef5d32b5c8e16b253ffb71254746f264496424bff31b2e03f42a8db3927db`,
28 rows; server PID 3209312 recorded graceful exit. The recipe removed its owned
temporary directory after printing this drained provenance.

“Observed” means retained payload proves that change shape; it does not prove
which user action caused it. Actions count as captured only with native action
provenance; unavailable actions remain explicit.

`cargo test --test wirelog` parses the three action sidecars and checks exact
action/row counts and ordering, complete selected edit ranges and payloads,
repair request/reply correlation, and rapid-repair text/version/hash
relationships. These are structural checks of committed evidence, not proof of
native causality inferred from a payload. Full-source SHA256 and byte-for-byte
equality against the original raw capture remain manual provenance checks:
full raw files are not committed.

| Native action | Helix 25.07.1 | Neovim traces |
|---|---|---|
| Single insert | Observed payload shape (Helix row 2); native single-key cause documented. | Native A insertion in plain trace row 3; retained. |
| Selection replace | Captured: native `ggghvllrX` replaced `Hea` with `XXX` (source row 10); server repair and echo restored baseline. | Captured: native `1G0vllrX<Esc>` produced `XXXd`; action trace metadata and source rows 9, 14 retained in `plain-neovim-0.12.6-actions.*`. |
| Paste one character | Captured: native `"ay` yank then `"ap` paste inserted `H` at 0:1 (source row 16); server repair and echo restored baseline. | Captured: native register `Q` and `gg0"ap` produced `HQead`; action trace metadata and source rows retained in `plain-neovim-0.12.6-actions.*`. |
| Paste many characters | Captured: native `vll"by` yank then `"bp` paste inserted `Hea` at 0:1 (source row 22); server repair and echo restored baseline. | Captured: native register `MULTI` and `gg0"ap` produced `HMULTIQead`; it began before prior repair settled. See retained action metadata and limits. |
| Rapid typing | NOT CAPTURED; separate rapid input not tested. | Captured: native `gg0iRAPID<Esc>`; raw change and metadata retained in `plain-neovim-0.12.6-rapid-repair.*`. |
| Undo immediately after server repair edit | NOT CAPTURED. | Captured: native `u` sent only after correlated repair reply and baseline buffer restoration; actual undo and follow-on server repair retained in `plain-neovim-0.12.6-rapid-repair.*`. |

Neovim action capture source is `/tmp/opencode/t002-neovim-actions/raw-parent/raw.jsonl`, SHA256 `856b70d34cb40b2d221177f89d43b75399adaf8bfa9d543253ace4e5e20360de` (21 rows). `plain-neovim-0.12.6-actions.jsonl` preserves source rows 9 and 14 verbatim; its JSON sidecar records exact native keys, structured before/after text and versions, and source row mapping. Source row 9 is didChange version 5 (`XXX` replacement); row 14 is didChange version 9 and contains Q and MULTI edits together with intervening server edits. Native state versions were 7 and 8 for the two pastes. MULTI began before prior repair settled, so this is action evidence, not a clean-repair claim. The run also executed incidental undo: its `repair_before_undo` still contains `HMULTIQead`, so undo-after-repair is not established and is excluded from the action fixture.

The original driver and report are retained at `/tmp/opencode/t002-neovim-actions/driver.py` and `/tmp/opencode/t002-neovim-actions/report.json`. The capture was run with `python3 /tmp/opencode/t002-neovim-actions/driver.py`; the recorded driver lines 71–79 still enabled the incidental undo. Do not treat this command as a three-action-only run or rerun it to recreate this evidence.

Reproduce the three native actions on Linux from any checkout subdirectory with
Neovim 0.12.6, tmux, Python 3 and Git. This standalone recipe builds the debug
server if needed, creates isolated HOME/repository/socket/tmux state, records
structured native before/after states, waits for each real server repair before
the next action, and retains all evidence under its printed temporary path.
Fresh extraction is compared to that run's source, not historical PID, URI or
timestamps; it validates this method and action causality, not the historical
source. The historical MULTI action began before its prior repair settled; this
fresh recipe makes no claim about old MULTI's repair boundary.

```sh
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
python3 - "$ROOT" <<'PY'
import hashlib, json, os, pathlib, shutil, signal, subprocess, sys, time
root=pathlib.Path(sys.argv[1]).resolve(); tag=f'{os.getpid()}-{time.time_ns()}'
d=pathlib.Path('/tmp/opencode')/f't002-neovim-actions-{tag}'; repo=d/'repo'; raw=d/'raw-parent/raw.jsonl'
sock=pathlib.Path('/tmp')/f't002-neovim-actions-{tag}.sock'; label=f't002-actions-{tag}'; session='cap'; pid=None
def run(a,**kw): return subprocess.run(a,text=True,capture_output=True,check=True,**kw)
def rows():
 if not raw.exists(): return []
 out=[]
 for line in raw.read_text().splitlines():
  try: out.append(json.loads(line))
  except json.JSONDecodeError: continue
 return out
def rpc(expr): return run(['nvim','--server',str(sock),'--remote-expr',expr],timeout=3).stdout.strip()
def state():
 lua="(function() local b=vim.api.nvim_get_current_buf(); return vim.json.encode({text=table.concat(vim.api.nvim_buf_get_lines(b,0,-1,false),'\\n'),version=vim.lsp.util.buf_versions[b],name=vim.api.nvim_buf_get_name(b)}) end)()"
 return json.loads(rpc('luaeval('+json.dumps(lua)+')'))
def wait(label,pred,seconds=20):
 end=time.monotonic()+seconds; last=None
 while time.monotonic()<end:
  try:
   x=state(); last=x
   if pred(x): return x
  except (subprocess.CalledProcessError, json.JSONDecodeError): pass
  time.sleep(.05)
 raise RuntimeError(f'timeout {label}; last state={last!r}')
def changes_after(offset,version):
 return [i for i,r in enumerate(rows()[offset:],offset) if r.get('direction')=='in' and r.get('raw',{}).get('method')=='textDocument/didChange' and r.get('raw',{}).get('params',{}).get('textDocument',{}).get('version')==version]
report={'actions':[]}
d.mkdir(parents=True); (d/'home').mkdir(); repo.mkdir(); raw.parent.mkdir()
try:
 run(['git','init','-q'],cwd=repo); run(['git','config','user.name','Capture'],cwd=repo); run(['git','config','user.email','capture@example.invalid'],cwd=repo)
 (repo/'tracked.txt').write_text('base\n'); run(['git','add','tracked.txt'],cwd=repo); run(['git','commit','-qm','baseline'],cwd=repo); (repo/'.magit').write_text('sentinel\n')
 server=root/'target/debug/magit-lsp'
 if not server.exists(): run(['cargo','build','--manifest-path',str(root/'Cargo.toml')],cwd=root,timeout=170)
 env=os.environ.copy(); env.update(HOME=str(d/'home'),XDG_CONFIG_HOME=str(d/'home/config'),XDG_DATA_HOME=str(d/'home/data'),XDG_STATE_HOME=str(d/'home/state'),GIT_CONFIG_NOSYSTEM='1',GIT_TERMINAL_PROMPT='0',MAGIT_LSP_WIRELOG=str(raw),PLAIN_REPO=str(repo),PLAIN_SERVER=str(server))
 run(['tmux','-L',label,'new-session','-d','-x','120','-y','50','-s',session,'nvim','--listen',str(sock),'-u',str(root/'tests/ui/plain_init.lua'),str(repo/'.magit')],cwd=repo,env=env)
 base=wait('initialized rendered status',lambda x:x['version'] is not None and 'Head:' in x['text'])
 init=next(r for r in rows() if r.get('raw',{}).get('method')=='initialize'); pid=init['pid']
 def act(name,keys,pred,repair=True):
  before=state(); offset=len(rows()); run(['nvim','--server',str(sock),'--remote-send',keys],timeout=8); after=wait(name,pred)
  deadline=time.monotonic()+15
  while time.monotonic()<deadline and not changes_after(offset,after['version']): time.sleep(.05)
  if not changes_after(offset,after['version']): raise RuntimeError(f'{name}: didChange boundary missing')
  rec={'action':name,'keys':keys,'before':before,'after':after,'didChange_row':changes_after(offset,after['version'])[0]+1}
  if repair:
   fixed=wait(name+' authoritative repair',lambda x:x['version']>after['version'] and x['text']==base['text'])
   rec['repair'] = fixed
  report['actions'].append(rec); return after
 lines=base['text'].splitlines(); line=next(i for i,x in enumerate(lines) if 'Head:' in x)+1
 act('selection replacement',f'{line}G0vllrX<Esc>',lambda x:x['version']>base['version'] and x['text']!=base['text'])
 # Q and MULTI are assigned through native Ex input; setup does not edit the buffer.
 run(['nvim','--server',str(sock),'--remote-send','<Cmd>let @a="Q"<CR>'],timeout=8)
 s=state(); act('one-character register paste','gg0"ap',lambda x:x['version']>s['version'] and x['text']!=s['text'])
 run(['nvim','--server',str(sock),'--remote-send','<Cmd>let @a="MULTI"<CR>'],timeout=8)
 s=state(); act('multi-character register paste','gg0"ap',lambda x:x['version']>s['version'] and x['text']!=s['text'])
 # Drain only after graceful editor close and recorded server exit.
 run(['nvim','--server',str(sock),'--remote-send','<Cmd>qa!<CR>'],timeout=8)
 deadline=time.monotonic()+20
 while time.monotonic()<deadline:
  rs=rows(); exits=[r for r in rs if r.get('pid')==pid and r.get('raw',{}).get('method') in ('shutdown','exit')]
  try: os.kill(pid,0); alive=True
  except ProcessLookupError: alive=False
  if any(r.get('raw',{}).get('method')=='exit' for r in exits) and not alive: break
  time.sleep(.05)
 else: raise RuntimeError('owned LSP server did not drain after graceful editor exit')
 report['server_pid']=pid; report['server_exit_observed']=True
 # Extraction selects initialize and didChange rows for this run's recorder only.
 selected=[r for r in rows() if r.get('direction')=='in' and r.get('pid')==pid and r.get('raw',{}).get('method') in ('initialize','textDocument/didChange')]
 (d/'generated.jsonl').write_text(''.join(json.dumps(r,separators=(',',':'))+'\n' for r in selected))
 # Verify parsed extraction against parsed source rows and final drained source hash.
 parsed=[json.loads(x) for x in (d/'generated.jsonl').read_text().splitlines()]
 assert parsed==selected and len([r for r in selected if r.get('raw',{}).get('method')=='textDocument/didChange'])>=3
 report.update(source=str(raw),source_sha256=hashlib.sha256(raw.read_bytes()).hexdigest(),source_rows=len(rows()),selected_rows=len(selected),extraction_matches_source=True,actions_count=3)
 (d/'report.json').write_text(json.dumps(report,indent=2)+'\n'); print((d/'report.json').read_text())
finally:
 # Preserve failed run evidence. Always release only this run's tmux/socket/server.
 subprocess.run(['tmux','-L',label,'kill-server'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 deadline=time.monotonic()+5
 while sock.exists() and time.monotonic()<deadline: time.sleep(.05)
 if pid:
  deadline=time.monotonic()+5
  while time.monotonic()<deadline:
   try: os.kill(pid,0)
   except ProcessLookupError: break
   time.sleep(.05)
 if sock.exists(): sock.unlink()
 if 'error' in report: print(f'FAILED evidence retained: {d}',file=sys.stderr)
if sock.exists(): raise RuntimeError('owned Neovim socket remains')
if subprocess.run(['tmux','-L',label,'has-session','-t',session],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0: raise RuntimeError('owned tmux session remains')
if pid:
 try: os.kill(pid,0)
 except ProcessLookupError: pass
 else: raise RuntimeError(f'owned server PID remains: {pid}')
print(f'cleanup verified; evidence={d}')
PY
```

The separate rapid/undo capture is retained in `plain-neovim-0.12.6-rapid-repair.jsonl` and its metadata sidecar. Final drained raw source SHA256 is `05295fce72b5d9da6f2c57d62648b77d9fd95aa84a6e3b9a28800882d07c6397` (21 rows, including shutdown, response and exit); source rows 9, 10, 13 and 14 are copied byte-for-byte. Row 9 proves rapid native input; row 10 is the new server `workspace/applyEdit` deleting `RAPID`; row 13 is its correlated editor reply. The reply is not accepted alone: native buffer text and version were inspected and matched the exact baseline before sending `u`. Row 14 coalesces the server repair echo (delete `RAPID`) with the native undo (insert `RAPID`); there is no separate echo before undo. The observed undo restored `RAPID`, then the server issued a further repair. The sidecar records baseline/after hashes, versions, native keys, row offsets and request ID without prescribing the undo outcome.

Reproduce on Linux from any checkout subdirectory with Neovim 0.12.6, tmux, Python 3 and Git. This complete recipe creates a real temporary Git repository, uses a unique recorder/socket/tmux namespace, retries startup remote-expr errors with a deadline, polls wire/state conditions, sends undo only after matching correlated reply and baseline restoration, gracefully closes Neovim, waits for the server's exit and process termination, then prints final source hash/count. It preserves the raw capture directory for audit.

```sh
ROOT=$(git rev-parse --show-toplevel)
python3 - "$ROOT" <<'PY'
import hashlib, json, os, pathlib, subprocess, sys, time
root=pathlib.Path(sys.argv[1]).resolve(); tag=f'{os.getpid()}-{time.time_ns()}'; d=pathlib.Path('/tmp/opencode')/f't002-neovim-rapid-repair-{tag}'
repo=d/'repo'; raw=d/'raw-parent/raw.jsonl'; sock=f'/tmp/t002-neovim-rapid-repair-{tag}.sock'; label=f't002-rapid-{tag}'
def run(a,**kw): return subprocess.run(a,text=True,capture_output=True,check=True,**kw)
def rows(): return [json.loads(x) for x in raw.read_text().splitlines() if x.strip()] if raw.exists() else []
def state():
 lua="(function() local b=vim.api.nvim_get_current_buf(); return vim.json.encode({text=table.concat(vim.api.nvim_buf_get_lines(b,0,-1,false),'\\n'),version=vim.lsp.util.buf_versions[b],name=vim.api.nvim_buf_get_name(b)}) end)()"
 end=time.monotonic()+8
 while time.monotonic()<end:
  try: return json.loads(run(['nvim','--server',sock,'--remote-expr','luaeval('+json.dumps(lua)+')'],timeout=2).stdout)
  except subprocess.CalledProcessError: time.sleep(.05)
 raise RuntimeError('Neovim remote-expr readiness timeout')
def wait(pred,label,seconds=20):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  try:
   x=state()
   if pred(x): return x
  except subprocess.CalledProcessError: pass
  time.sleep(.05)
 raise RuntimeError('timeout: '+label)
d.mkdir(parents=True); repo.mkdir(); (d/'home').mkdir(); raw.parent.mkdir()
try:
 run(['git','init','-q'],cwd=repo); run(['git','config','user.name','Capture'],cwd=repo); run(['git','config','user.email','capture@example.invalid'],cwd=repo)
 (repo/'tracked').write_text('base\n'); run(['git','add','tracked'],cwd=repo); run(['git','commit','-qm','base'],cwd=repo); (repo/'.magit').write_text('sentinel\n')
 env=os.environ.copy(); env.update(HOME=str(d/'home'),XDG_CONFIG_HOME=str(d/'home/config'),XDG_DATA_HOME=str(d/'home/data'),XDG_STATE_HOME=str(d/'home/state'),GIT_CONFIG_NOSYSTEM='1',GIT_TERMINAL_PROMPT='0',MAGIT_LSP_WIRELOG=str(raw),PLAIN_REPO=str(repo),PLAIN_SERVER=str(root/'target/debug/magit-lsp'))
 if not pathlib.Path(env['PLAIN_SERVER']).exists(): run(['cargo','build','--manifest-path',str(root/'Cargo.toml')],cwd=root,timeout=180)
 subprocess.run(['tmux','-L',label,'new-session','-d','-x','120','-y','50','-s','cap','nvim','--listen',sock,'-u',str(root/'tests/ui/plain_init.lua'),str(repo/'.magit')],cwd=repo,env=env,check=True)
 base=wait(lambda x:x['version'] is not None and 'Head:' in x['text'],'initial render'); offset=len(rows()); run(['nvim','--server',sock,'--remote-send','gg0iRAPID<Esc>']); changed=wait(lambda x:x['text']!=base['text'],'rapid input')
 end=time.monotonic()+20; req=None
 while time.monotonic()<end:
  wire=rows(); req=next(((i,r) for i,r in enumerate(wire[offset:],offset) if r.get('direction')=='out' and r.get('raw',{}).get('method')=='workspace/applyEdit' and any(u.endswith('/.magit') and any(c.get('newText')=='' for c in cs) for u,cs in r.get('raw',{}).get('params',{}).get('edit',{}).get('changes',{}).items())),None)
  if req: break
  time.sleep(.05)
 if not req: raise RuntimeError('no new repair applyEdit')
 rid=req[1]['raw']['id']; end=time.monotonic()+20; reply=None
 while time.monotonic()<end:
  wire=rows(); reply=next(((i,r) for i,r in enumerate(wire[req[0]+1:],req[0]+1) if r.get('direction')=='in' and r.get('raw',{}).get('id')==rid),None)
  if reply: break
  time.sleep(.05)
 if not reply: raise RuntimeError('no correlated applyEdit reply')
 restored=wait(lambda x:x['text']==base['text'] and x['version']>changed['version'],'native repair restoration'); before=state(); run(['nvim','--server',sock,'--remote-send','u']); undone=wait(lambda x:x['version']>before['version'],'native undo')
 end=time.monotonic()+10
 while time.monotonic()<end:
  wire=rows(); echo=next(((i,r) for i,r in enumerate(wire) if r.get('direction')=='in' and r.get('raw',{}).get('method')=='textDocument/didChange' and r.get('raw',{}).get('params',{}).get('textDocument',{}).get('version')==undone['version']),None)
  if echo: break
  time.sleep(.05)
 if not echo: raise RuntimeError('undo didChange not recorded')
finally:
 subprocess.run(['nvim','--server',sock,'--remote-send','<Cmd>qa!<CR>'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 end=time.monotonic()+15
 while time.monotonic()<end:
  wire=rows(); init=next((r for r in wire if r.get('raw',{}).get('method')=='initialize'),None); pid=init.get('pid') if init else None
  exited=pid and any(r.get('pid')==pid and r.get('raw',{}).get('method')=='exit' for r in wire)
  try: os.kill(pid,0); alive=True
  except (ProcessLookupError,TypeError): alive=False
  if exited and not alive: break
  time.sleep(.05)
 else: raise RuntimeError('owned LSP server did not gracefully exit')
 subprocess.run(['tmux','-L',label,'kill-server'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 if pathlib.Path(sock).exists(): raise RuntimeError('owned Neovim socket remains')
 wire=rows(); (d/'report.json').write_text(json.dumps({'source':str(raw),'source_sha256':hashlib.sha256(raw.read_bytes()).hexdigest(),'source_rows':len(wire),'rapid':changed,'repair_request_id':rid,'repair_request_row':req[0]+1,'reply_row':reply[0]+1,'restored':restored,'undo':undone,'undo_echo_row':echo[0]+1,'server_pid':pid,'server_exit_observed':bool(exited),'socket_remains':False},indent=2)+'\n')
 print((d/'report.json').read_text())
PY
```

The equivalent capture driver is retained at `/tmp/opencode/t002-neovim-rapid-repair/driver.py`; neither it nor that path is needed by the README recipe. To regenerate fixture rows and metadata, select rapid didChange, following `workspace/applyEdit`, matching response ID, and undo didChange in source order. Compare retained JSONL lines byte-for-byte with source lines, then record source hash and structured before/after state. Checked extraction indices were `[8, 9, 12, 13]` (zero-based); the final source has 21 rows after graceful drain.

Plain Neovim row 2 is a server-rendered insertion, row 3 native `A`
insertion, and row 4 `A` deletion/repair; only row 3 establishes single insert.
Bundled Neovim's commit-draft change is a native full-buffer replacement, not
proof of any particular requested input action. Bundled VSCode's retained
change is also a full-buffer replacement; no native action among the six is
established there. Single insert is captured for both requested editor families.
For Neovim, selection replacement, both register pastes, rapid typing, and undo
after confirmed repair are captured. For Helix, selection replacement and both
register pastes are captured; rapid typing and undo after confirmed repair remain
uncaptured. The checked
T002.2 four-client baseline is complete; parent T002 and full T034 conformance
remain open.
