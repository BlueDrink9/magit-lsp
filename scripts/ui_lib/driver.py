"""tmux/Neovim driver behind `scripts/ui` (spec §A.1)."""

from __future__ import annotations

import json
import os
import re
import secrets
import shlex
import shutil
import subprocess
import time
from dataclasses import asdict, dataclass
from pathlib import Path

from . import fixtures
from .paths import BUILD_UI, LAST_RUN, MINIMAL_INIT, RUNS, run_dir

SESSION = "magit"
DEFAULT_SIZE = (100, 30)
DEFAULT_PROFILE = "vim"
START_TIMEOUT_MS = 20000
SOCKET_TIMEOUT_MS = 15000


class DriverError(RuntimeError):
    """A driver command could not be completed."""


class SkipError(DriverError):
    """The environment lacks a required tool."""


@dataclass
class Run:
    """Everything one `ui start` created."""

    id: str
    fixture: str
    profile: str
    cols: int
    rows: int
    color: str
    directory: str
    repo: str
    home: str
    socket: str
    trace: str
    tmux: str

    @property
    def dir(self) -> Path:
        """The run's state directory."""
        return Path(self.directory)

    @property
    def repo_path(self) -> Path:
        """The fixture repository."""
        return Path(self.repo)

    @property
    def home_path(self) -> Path:
        """The isolated HOME used by git and Neovim."""
        return Path(self.home)

    @property
    def socket_path(self) -> Path:
        """The Neovim RPC socket."""
        return Path(self.socket)

    @property
    def trace_path(self) -> Path:
        """The `MAGIT_LSP_TRACE` file."""
        return Path(self.trace)

    def save(self) -> None:
        """Persist this run so later commands can find it."""
        state = self.dir / "state.json"
        state.write_text(json.dumps(asdict(self), indent=2), encoding="utf-8")

    @classmethod
    def load(cls, run_id: str) -> "Run":
        """Load a saved run."""
        state = run_dir(run_id) / "state.json"
        if not state.is_file():
            raise DriverError(f"unknown run `{run_id}`")
        try:
            data = json.loads(state.read_text(encoding="utf-8"))
            return cls(**data)
        except (TypeError, ValueError) as error:
            raise DriverError(f"corrupt run state in {state}: {error}") from error


def check_tools() -> None:
    """Raise `SkipError` when tmux or nvim is unavailable."""
    for tool in ("tmux", "nvim"):
        if shutil.which(tool) is None:
            raise SkipError(f"{tool} not found")


def parse_size(value: str) -> tuple[int, int]:
    """Parse `WxH` into a positive `(cols, rows)` pair."""
    match = re.fullmatch(r"(\d+)x(\d+)", value or "")
    if not match:
        raise DriverError(f"invalid size `{value}`; expected WxH")
    cols, rows = int(match.group(1)), int(match.group(2))
    if cols <= 0 or rows <= 0:
        raise DriverError(f"invalid size `{value}`")
    return cols, rows


def new_run_id() -> str:
    """Return a timestamped run id that is unique across parallel starts."""
    stamp = time.strftime("%Y%m%d-%H%M%S", time.gmtime())
    return f"{stamp}-{secrets.token_hex(3)}"


def _update_last_run(run_id: str) -> None:
    BUILD_UI.mkdir(parents=True, exist_ok=True)
    LAST_RUN.write_text(run_id, encoding="utf-8")


def latest_run_id() -> str:
    """Return the most recent run id, or raise."""
    if LAST_RUN.is_file():
        run_id = LAST_RUN.read_text(encoding="utf-8").strip()
        if run_id and (run_dir(run_id) / "state.json").is_file():
            return run_id
    if RUNS.is_dir():
        states = [path for path in RUNS.iterdir() if (path / "state.json").is_file()]
        if states:
            return max(states, key=lambda path: path.stat().st_mtime).name
    raise DriverError("no run found; start one with `scripts/ui start <fixture>`")


def load_run(run_id: str | None) -> Run:
    """Load `run_id`, defaulting to the most recent run."""
    return Run.load(run_id or latest_run_id())


def _forget_last_run(run_id: str) -> None:
    if LAST_RUN.is_file() and LAST_RUN.read_text(encoding="utf-8").strip() == run_id:
        remaining = []
        if RUNS.is_dir():
            remaining = [
                path for path in RUNS.iterdir() if (path / "state.json").is_file()
            ]
        if remaining:
            newest = max(remaining, key=lambda path: path.stat().st_mtime).name
            _update_last_run(newest)
        else:
            LAST_RUN.unlink(missing_ok=True)


def _tmux(run: Run, *args: str, check: bool = True) -> subprocess.CompletedProcess:
    result = subprocess.run(
        ["tmux", "-L", run.tmux, *args],
        capture_output=True,
        text=True,
    )
    if check and result.returncode != 0:
        raise DriverError(
            f"tmux {' '.join(args)} failed: {result.stderr.strip() or result.stdout.strip()}"
        )
    return result


def _nvim(run: Run, *args: str, timeout: float = 5.0) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["nvim", "--server", run.socket, *args],
        capture_output=True,
        text=True,
        timeout=timeout,
    )


def _expr(run: Run, expression: str, timeout: float = 5.0) -> str | None:
    try:
        result = _nvim(run, "--remote-expr", expression, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode != 0:
        return None
    return result.stdout.strip()


_SNAPSHOT_EXPR = (
    'luaeval(\'vim.json.encode(require("magit-lsp.debug").snapshot())\')'
)
_IDLE_EXPR = 'luaeval(\'require("magit-lsp.debug").idle()\')'


def snapshot(run: Run) -> dict:
    """Return the live `snapshot()` object."""
    output = _expr(run, _SNAPSHOT_EXPR)
    if not output:
        raise DriverError("Neovim did not answer the state request")
    try:
        return json.loads(output)
    except ValueError as error:
        raise DriverError(f"invalid state from Neovim: {output!r}") from error


def state_json(run: Run) -> str:
    """Pretty JSON for `ui state` and timeout reports."""
    return json.dumps(snapshot(run), indent=2)


def idle(run: Run) -> bool:
    """Poll the client's `debug.idle()`."""
    output = _expr(run, _IDLE_EXPR, timeout=2.0)
    return output in ("true", "1")


def wait_idle(run: Run, timeout_ms: int = 2000, settle_ms: int = 50) -> bool:
    """Wait for idle, allowing `settle_ms` for queued input to be consumed.

    Two `true` readings at least `settle_ms` apart are required: a key that
    tmux has just written is still in Neovim's input queue when the first
    poll runs, and reporting idle then would race the key.
    """
    deadline = time.monotonic() + timeout_ms / 1000
    first_true: float | None = None
    while time.monotonic() < deadline:
        if idle(run):
            now = time.monotonic()
            if first_true is None:
                first_true = now
            if (now - first_true) * 1000 >= settle_ms:
                return True
        else:
            first_true = None
        time.sleep(0.02)
    return False


def capture(run: Run, ansi: bool = False) -> str:
    """Return the pane text, padded to exactly `rows` lines."""
    args = ["capture-pane", "-p", "-t", SESSION]
    if ansi:
        args.insert(2, "-e")
    result = _tmux(run, *args)
    lines = result.stdout.splitlines()
    lines = (lines + [""] * run.rows)[: run.rows]
    return "\n".join(lines)


_TMUX_KEY = re.compile(
    r"^(?:"
    r"(?:C|M|S)-(?:[A-Za-z0-9]|\\(?:\\||\[|\]|/))"
    r"|F(?:[1-9]|1[0-2])"
    r"|Escape|Esc|Enter|CR|Tab|BTab|BSpace|Space"
    r"|Delete|DC|Insert|IC|Home|End|PPage|NPage|Up|Down|Left|Right"
    r")$"
)


def is_key_name(token: str) -> bool:
    """True when `token` should be sent as a tmux key name, not literal text."""
    return bool(_TMUX_KEY.fullmatch(token))


def send_keys(run: Run, tokens: list[str]) -> None:
    """Send each token to the pane, literal text or named key."""
    for token in tokens:
        if is_key_name(token):
            _tmux(run, "send-keys", "-t", SESSION, token)
        else:
            _tmux(run, "send-keys", "-t", SESSION, "-l", "--", token)


def _pane_dead(run: Run) -> bool:
    result = _tmux(
        run, "display-message", "-p", "-t", SESSION, "#{pane_dead}", check=False
    )
    return result.returncode != 0 or result.stdout.strip() == "1"


def _wait_for_socket(run: Run, timeout_ms: int = SOCKET_TIMEOUT_MS) -> None:
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        if run.socket_path.exists() and idle(run):
            return
        if _pane_dead(run):
            raise DriverError(f"Neovim exited during startup:\n{capture(run)}")
        time.sleep(0.025)
    raise DriverError(f"Neovim did not open {run.socket_path}:\n{capture(run)}")


def _wait_for_view(run: Run, timeout_ms: int = START_TIMEOUT_MS) -> None:
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        try:
            state = snapshot(run)
        except DriverError:
            state = None
        if (
            state
            and state.get("view_stack")
            and (state.get("render") or {}).get("version", 0) > 0
            and state.get("idle")
        ):
            return
        if _pane_dead(run):
            raise DriverError(f"Neovim exited during startup:\n{capture(run)}")
        time.sleep(0.025)
    raise DriverError(f"the status view did not render in time:\n{capture(run)}")


def _cleanup(run: Run) -> None:
    """Remove every resource a run created; safe to call repeatedly."""
    try:
        _tmux(run, "kill-server", check=False)
    except OSError:
        pass
    shutil.rmtree(run.dir, ignore_errors=True)
    _forget_last_run(run.id)


def _nvim_command(run: Run, env: dict[str, str]) -> str:
    nvim = shutil.which("nvim") or "nvim"
    # Work around the known readonly warning (R-UI-3, bug 4) just enough that
    # the first render cannot block startup with W10. The client fix drops
    # `readonly`; until then the harness cannot open a view without this.
    ignore_readonly = (
        "lua vim.api.nvim_create_autocmd('FileChangedRO', {"
        " callback = function()"
        " if vim.bo.filetype == 'magit-lsp' then vim.bo.readonly = false end"
        " end })"
    )
    args = [
        "env",
        "-i",
        *[f"{key}={value}" for key, value in sorted(env.items())],
        nvim,
        "--listen",
        str(run.socket_path),
        "-u",
        str(MINIMAL_INIT),
        "-i",
        "NONE",
        "-n",
        "--cmd",
        "set ttimeoutlen=0",
        "--cmd",
        ignore_readonly,
    ]
    return shlex.join(args)


def start(
    fixture: str,
    size: str = f"{DEFAULT_SIZE[0]}x{DEFAULT_SIZE[1]}",
    profile: str = DEFAULT_PROFILE,
    color: str = "ansi",
) -> Run:
    """Build a fixture, start Neovim in tmux and wait for the first render."""
    check_tools()
    if profile not in ("standard", "vim"):
        raise DriverError(f"unknown profile `{profile}`; expected standard or vim")
    cols, rows = parse_size(size)
    run_id = new_run_id()
    directory = run_dir(run_id)
    run = Run(
        id=run_id,
        fixture=fixture,
        profile=profile,
        cols=cols,
        rows=rows,
        color=color,
        directory=str(directory),
        repo=str(directory / "repo"),
        home=str(directory / "home"),
        socket=str(directory / "nvim.sock"),
        trace=str(directory / "trace.jsonl"),
        tmux=f"magit-ui-{run_id}",
    )
    try:
        directory.mkdir(parents=True)
        try:
            fixtures.build(fixture, run.repo_path, run.home_path)
        except (OSError, RuntimeError, ValueError) as error:
            raise DriverError(f"could not build fixture `{fixture}`: {error}") from error
        run.trace_path.touch()
        run.save()

        env = fixtures.determinism_env(run.home_path)
        env.update(
            {
                "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
                "TERM": "xterm-256color",
                "COLORTERM": "truecolor" if color == "truecolor" else "",
                "NVIM_APPNAME": "magit-ui",
                "MAGIT_LSP_PROFILE": profile,
                "MAGIT_LSP_TRACE": str(run.trace_path),
                "MAGIT_LSP_UI_ROOT": str(run.repo_path),
            }
        )

        _tmux(
            run,
            "new-session",
            "-d",
            "-s",
            SESSION,
            "-x",
            str(cols),
            "-y",
            str(rows),
            "-c",
            str(run.repo_path),
        )
        _tmux(run, "set", "-g", "escape-time", "0")
        _tmux(run, "set", "-g", "status", "off")
        _tmux(run, "set", "-g", "default-terminal", "xterm-256color")
        # The options above must exist before Neovim starts, so the initial
        # shell is replaced once the server is configured.
        _tmux(
            run,
            "respawn-pane",
            "-k",
            "-t",
            SESSION,
            "-c",
            str(run.repo_path),
            _nvim_command(run, env),
        )

        _wait_for_socket(run)
        _wait_for_view(run)
        _update_last_run(run.id)
        return run
    except BaseException:
        _cleanup(run)
        raise


def stop(run: Run) -> None:
    """Quit Neovim, kill its tmux server and delete the run's files."""
    if run.socket_path.exists():
        try:
            _nvim(
                run,
                "--remote-send",
                "<C-\\><C-N>:qa!<CR>",
                timeout=3.0,
            )
        except (OSError, subprocess.TimeoutExpired):
            pass
        deadline = time.monotonic() + 2.0
        while run.socket_path.exists() and time.monotonic() < deadline:
            time.sleep(0.02)
    try:
        _tmux(run, "kill-server", check=False)
    except OSError:
        pass
    shutil.rmtree(run.dir, ignore_errors=True)
    _forget_last_run(run.id)


def read_trace(run: Run) -> list[dict]:
    """Read the client trace file, ignoring a partially written last line."""
    events: list[dict] = []
    if not run.trace_path.is_file():
        return events
    for line in run.trace_path.read_text(encoding="utf-8").splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            continue
    return events


def format_trace(events: list[dict], count: int | None = None) -> str:
    """Format the last `count` key traces with per-hop deltas."""
    groups: list[list[dict]] = []
    current_id = object()
    for event in events:
        if event.get("id") != current_id:
            groups.append([])
            current_id = event.get("id")
        groups[-1].append(event)
    if count is not None:
        groups = groups[-count:] if count > 0 else []
    lines: list[str] = []
    for group in groups:
        previous: float | None = None
        for event in group:
            when = float(event.get("t_ms", 0.0))
            delta = 0.0 if previous is None else when - previous
            previous = when
            detail = event.get("detail")
            suffix = "" if detail in (None, {}) else " " + json.dumps(detail)
            lines.append(
                f"#{event.get('id')} +{delta:8.3f}ms {event.get('event')}{suffix}"
            )
    return "\n".join(lines)
