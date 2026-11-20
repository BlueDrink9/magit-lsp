"""tmux/Neovim driver behind `scripts/ui` (spec §A.1)."""

from __future__ import annotations

import base64
import contextlib
import json
import os
import re
import secrets
import signal
import shlex
import shutil
import subprocess
import sys
import time
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path

from . import fixtures, keys, startup
from .paths import BUILD_UI, LAST_RUN, MINIMAL_INIT, NATIVE_SHUTDOWN, PLAIN_INIT, RUNS, run_dir, run_socket_path

SESSION = "magit"
DEFAULT_SIZE = (100, 30)
DEFAULT_PROFILE = "vim"
START_TIMEOUT_MS = 20000
SOCKET_TIMEOUT_MS = 15000
SCREEN_POLL_MS = 50
SCREEN_TIMEOUT_MS = 2000
TMUX_TIMEOUT = 5.0


class DriverError(RuntimeError):
    """A driver command could not be completed."""


class StartupError(DriverError):
    """Neovim or its RPC socket did not come up.

    Kept distinct from scenario assertion failures so a harness startup problem
    is never reported as a behaviour regression (and vice versa).
    """


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
    client: str = "rich"

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
    return f"{stamp}-{secrets.token_hex(8)}"


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
    """Run one tmux command with a finite timeout."""
    result = subprocess.run(
        ["tmux", "-L", run.tmux, *args],
        capture_output=True,
        text=True,
        timeout=TMUX_TIMEOUT,
    )
    if check and result.returncode != 0:
        raise DriverError(
            f"tmux {' '.join(args)} failed: {result.stderr.strip() or result.stdout.strip()}"
        )
    return result


def _nvim(run: Run, *args: str, timeout: float = 5.0) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["nvim", "--headless", "--server", run.socket, *args],
        stdin=subprocess.DEVNULL,
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
_STARTUP_DEBUG_EXPR = (
    "luaeval('vim.json.encode((function(d) "
    "local views = {}; for _, id in ipairs(d.order) do "
    "local v = d.views[id]; views[#views + 1] = {id = id, kind = v.kind, "
    "version = v.version, acked = v.acked}; end; "
    "local progress = 0; for _ in pairs(d.state.progress) do "
    "progress = progress + 1; end; "
    "return {state = {requests = d.state.requests, queued = d.state.queued, "
    "prompts = d.state.prompts, scheduled = d.state.scheduled, "
    'progress = progress}, views = views, messages = vim.fn.execute("messages")}; '
    "end)(require(\"magit-lsp.debug\")))')"
)
# `vim.ui.input` uses the `@` command-line type and `vim.ui.select`/`inputlist`
# the `-` textlist type; both mean a prompt is waiting for the user.
_PROMPT_EXPR = "getcmdtype()"
PROMPT_TYPES = ("@", "-")
# JSON keeps line breaks and leading whitespace intact, so an exact comparison
# does not depend on how `--remote-expr` prints a multi-line value.
_DOCUMENT_EXPR = (
    'luaeval(\'vim.json.encode(table.concat('
    'vim.api.nvim_buf_get_lines(0, 0, -1, false), string.char(10)))\')'
)
# The search text travels as base64 so neither quoting level needs escaping.
_GOTO_EXPR = (
    "luaeval('(function(text)"
    " for index = 1, vim.api.nvim_buf_line_count(0) do"
    ' local line = vim.api.nvim_buf_get_lines(0, index - 1, index, false)[1] or ""'
    " local start = line:find(text, 1, true)"
    " if start then vim.api.nvim_win_set_cursor(0, {{ index, start - 1 }})"
    " return true end"
    ' end return false end)(vim.base64.decode("{}"))\')'
)


def snapshot(run: Run) -> dict:
    """Return the live `snapshot()` object."""
    expression = ('luaeval("vim.json.encode(PlainUI.snapshot())")'
                  if run.client == "plain" else _SNAPSHOT_EXPR)
    output = _expr(run, expression)
    if not output:
        raise DriverError("Neovim did not answer the state request")
    try:
        return json.loads(output)
    except ValueError as error:
        raise DriverError(f"invalid state from Neovim: {output!r}") from error


def state_json(run: Run, indent: int | None = 2) -> str:
    """Serialize state; pretty by default, compact when `indent=None`."""
    options = {"indent": indent}
    if indent is None:
        options["separators"] = (",", ":")
    return json.dumps(snapshot(run), **options)


def idle(run: Run) -> bool:
    """Poll the selected client's native idle observation."""
    return _idle_probe(run).outcome == "true"


def _idle_probe(run: Run) -> startup.IdleProbe:
    """Run one bounded idle expression and preserve its complete outcome."""
    try:
        expression = ('luaeval("PlainUI.idle()")'
                      if run.client == "plain" else _IDLE_EXPR)
        result = _nvim(run, "--remote-expr", expression, timeout=2.0)
    except subprocess.TimeoutExpired as error:
        return startup.IdleProbe(
            "timeout",
            stdout=_decode_output(error.stdout),
            stderr=_decode_output(error.stderr),
            detail="after 2s",
        )
    except OSError as error:
        return startup.IdleProbe("error", detail=str(error))
    stdout = result.stdout.strip()
    stderr = result.stderr.strip()
    if result.returncode != 0:
        return startup.IdleProbe("error", result.returncode, stdout, stderr)
    if stdout in ("true", "1"):
        return startup.IdleProbe("true", result.returncode, stdout, stderr)
    if stdout in ("false", "0"):
        return startup.IdleProbe("false", result.returncode, stdout, stderr)
    return startup.IdleProbe("unexpected", result.returncode, stdout, stderr)


def _rpc_probe(run: Run) -> startup.IdleProbe:
    """Check standard RPC availability without loading project Lua modules."""
    try:
        result = _nvim(run, "--remote-expr", "1", timeout=2.0)
    except subprocess.TimeoutExpired as error:
        return startup.IdleProbe("timeout", stdout=_decode_output(error.stdout),
                                 stderr=_decode_output(error.stderr), detail="after 2s")
    except OSError as error:
        return startup.IdleProbe("error", detail=str(error))
    stdout, stderr = result.stdout.strip(), result.stderr.strip()
    if result.returncode == 0 and stdout == "1":
        return startup.IdleProbe("true", result.returncode, stdout, stderr)
    return startup.IdleProbe("error", result.returncode, stdout, stderr)


def _decode_output(output: str | bytes | None) -> str:
    """Normalize `TimeoutExpired` output for bounded diagnostic reporting."""
    if isinstance(output, bytes):
        return output.decode("utf-8", errors="replace")
    return output or ""


def _client_startup_state(run: Run) -> list[str]:
    """Read and format bounded client busy state after startup has failed."""
    try:
        expression = ('luaeval("vim.json.encode(PlainUI.snapshot())")'
                      if run.client == "plain" else _STARTUP_DEBUG_EXPR)
        result = _nvim(run, "--remote-expr", expression, timeout=2.0)
    except (OSError, subprocess.TimeoutExpired) as error:
        return [f"client debug state: unavailable ({_safe_text(error)})"]
    if result.returncode != 0:
        detail = _safe_text(result.stderr or result.stdout)
        return [
            "client debug state: unavailable"
            + (f" (exit code {result.returncode}: {detail})" if detail else "")
        ]
    if len(result.stdout) > 16000:
        return ["client debug state: unavailable (diagnostic response exceeded limit)"]
    try:
        data = json.loads(result.stdout)
    except (TypeError, ValueError):
        return ["client debug state: unavailable (invalid diagnostic response)"]
    if run.client == "plain" and isinstance(data, dict):
        lines = [f"Plain client: ready={data.get('ready')}, idle={data.get('idle')}, "
                 f"applied edits={data.get('applied_edits')}",
                 f"active buffer: {_safe_text(data.get('uri', 'unknown'))}"]
        lines.extend(f"  {_safe_text(message)}" for message in data.get("messages", [])[-5:] if message.strip())
        return lines
    state = data.get("state", {}) if isinstance(data, dict) else {}
    progress = state.get("progress", 0)
    lines = [
        "client work: "
        + ", ".join(
            f"{name}={state.get(name, 0)}"
            for name in ("requests", "queued", "prompts", "scheduled")
        )
        + f", progress={progress}"
    ]
    views = data.get("views", [])
    if views:
        details = []
        for view in views[:8]:
            details.append(
                f"{_safe_text(view.get('id', '?'))} "
                f"({_safe_text(view.get('kind', '?'))}): "
                f"version={view.get('version', 'unset')}, "
                f"acked={view.get('acked', 'unset')}"
            )
        lines.append("views: " + "; ".join(details))
        if len(views) > 8:
            lines.append(f"views: … {len(views) - 8} more omitted")
    messages = data.get("messages", "")
    if isinstance(messages, str) and messages.strip():
        lines.append("recent Neovim messages:")
        lines.extend(
            f"  {_safe_text(message)}"
            for message in messages.splitlines()[-5:]
            if message.strip()
        )
    return lines


def _safe_text(value: object, limit: int = 400) -> str:
    """Collapse untrusted diagnostic text and cap its displayed length."""
    printable = "".join(char if char.isprintable() else " " for char in str(value))
    text = " ".join(printable.split())
    return text if len(text) <= limit else text[: limit - 3] + "..."


def document(run: Run) -> str:
    """Return the active buffer's text exactly, lines joined by a newline.

    Used to prove that a native edit (for example `J`) was refused because the
    document still has the same content; screen text alone is not enough.
    """
    output = _expr(run, _DOCUMENT_EXPR)
    if output is None:
        raise DriverError("Neovim did not answer the document request")
    try:
        text = json.loads(output)
    except ValueError as error:
        raise DriverError(f"invalid document from Neovim: {output!r}") from error
    if not isinstance(text, str):
        raise DriverError("Neovim returned a non-string document")
    return text


def goto_text(run: Run, text: str) -> None:
    """Move the cursor to the first buffer line containing `text`.

    The match is literal and the cursor lands on the first character, so a
    scenario can name its target instead of counting motions. It is an RPC
    call, not a key press, so it never contributes to a latency measurement.
    """
    payload = base64.b64encode(text.encode("utf-8")).decode("ascii")
    output = _expr(run, _GOTO_EXPR.format(payload))
    if output is None:
        raise DriverError("Neovim did not answer the goto request")
    if output not in ("true", "1"):
        raise DriverError(f"{text!r} is not in the active document")


def clear_message_line(run: Run) -> None:
    """Clear the command-line message display without touching `:messages`.

    `:mode` redraws the command line; a scenario still sees the recorded
    warning through `snapshot()["messages"]`.
    """
    _expr(run, 'execute("mode")')


def prompt_pending(run: Run) -> bool:
    """True while a `vim.ui` prompt is waiting for the user."""
    return _expr(run, _PROMPT_EXPR, timeout=1.0) in PROMPT_TYPES


def wait_prompt(run: Run, timeout_ms: int = 2000, poll_ms: int = 20) -> bool:
    """Poll until a prompt is waiting, or return False at the timeout.

    `idle()` stays false while a prompt is pending and the prompt blocks the
    key request that opened it, so a prompt scenario cannot use `wait_idle`.
    """
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        if prompt_pending(run):
            return True
        time.sleep(poll_ms / 1000)
    return prompt_pending(run)


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


def wait_screen(run: Run, text: str, timeout_ms: int = SCREEN_TIMEOUT_MS) -> bool:
    """Poll the pane until `text` appears, or return False at the timeout.

    A pending prompt blocks Neovim's RPC loop, so `idle()` cannot be the gate
    for a key that opens one. This waits on the visible screen instead, which
    keeps the wait bounded and independent of the prompt implementation.
    """
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        if text in capture(run):
            return True
        time.sleep(SCREEN_POLL_MS / 1000)
    return text in capture(run)


def parse_cursor(value: str) -> tuple[int, int | None]:
    """Parse a `row` or `row:col` barrier coordinate, both zero-based."""
    match = re.fullmatch(r"(\d+)(?::(\d+))?", value or "")
    if match is None:
        raise DriverError(f"invalid cursor `{value}`; expected row or row:col")
    row = int(match.group(1))
    col = int(match.group(2)) if match.group(2) is not None else None
    return row, col


def _view_matches(view: dict, wanted: str) -> bool:
    """Whether one snapshot view matches an id, its suffix, its title or kind."""
    candidates = [
        view.get("id"),
        view.get("title"),
        view.get("kind"),
        view.get("name"),
    ]
    for candidate in candidates:
        if candidate is None:
            continue
        text = str(candidate)
        if text == wanted:
            return True
        # Server view ids are only unique per LSP client; the client stores
        # `<client-id>\0<server-id>` and users normally name the server id.
        if "\0" in text and text.rsplit("\0", 1)[1] == wanted:
            return True
    return False


def _pending_barriers(
    run: Run,
    *,
    text: str | None,
    cursor: tuple[int, int | None] | None,
    view: str | None,
) -> list[str]:
    """Evaluate every requested barrier and return the unmet descriptions."""
    pending: list[str] = []
    if text is not None:
        try:
            screen = capture(run)
        except DriverError as error:
            pending.append(f"screen unavailable: {error}")
        else:
            if text not in screen:
                pending.append(f"screen text {text!r}")
    if cursor is not None or view is not None:
        try:
            state = snapshot(run)
        except DriverError as error:
            pending.append(f"state unavailable: {error}")
        else:
            if cursor is not None:
                row, col = cursor
                current = state.get("cursor") or {}
                actual_row = current.get("row")
                actual_col = current.get("col")
                if actual_row != row or (
                    col is not None and actual_col != col
                ):
                    pending.append(
                        f"cursor ({actual_row}, {actual_col}) at row {row}"
                        + (f" col {col}" if col is not None else "")
                    )
            if view is not None:
                stack = state.get("view_stack")
                if not isinstance(stack, list):
                    pending.append(f"view {view!r} (snapshot has no view_stack)")
                elif not any(_view_matches(entry, view) for entry in stack):
                    pending.append(f"view {view!r}")
    return pending


def wait_barriers(
    run: Run,
    *,
    text: str | None = None,
    cursor: tuple[int, int | None] | None = None,
    view: str | None = None,
    timeout_ms: int = 2000,
    poll_ms: int = 20,
) -> list[str]:
    """Poll until every requested barrier holds; return the unmet ones at timeout.

    An empty list means success. At least one barrier should be supplied; a
    caller with no barriers keeps using `wait_idle`. Unlike `wait_idle`, a
    barrier can observe a pending prompt through the screen or a structural
    snapshot, so `--text` is safe for prompts that block Neovim's RPC loop.
    """
    deadline = time.monotonic() + timeout_ms / 1000
    while True:
        pending = _pending_barriers(run, text=text, cursor=cursor, view=view)
        if not pending:
            return []
        if time.monotonic() >= deadline:
            return pending
        time.sleep(poll_ms / 1000)


def screen_excerpt(screen: str, limit: int = 12) -> str:
    """Trim blank rows and show at most `limit` lines with a middle ellipsis."""
    rows = [row.rstrip() for row in screen.splitlines()]
    while rows and not rows[-1]:
        rows.pop()
    if len(rows) <= limit:
        return "\n".join(rows)
    head = max((limit - 1) // 2, 0)
    tail = max(limit - 1 - head, 0)
    return "\n".join([*rows[:head], "…", *rows[-tail:]])


def is_key_name(token: str) -> bool:
    """True when `token` is sent as a tmux key name, not literal text."""
    return keys.encode(token).kind == "tmux"


def send_keys(run: Run, tokens: list[str]) -> None:
    """Send each token to the pane, literal text or named key.

    Modifier+special-key tokens (such as `C-Enter`) go through raw CSI-u
    bytes; plain tmux names would drop their modifier. Shifted Tab goes through
    tmux's backtab name so Neovim sees its `<S-Tab>` mapping. The table in
    `ui_lib.keys` decides which path each token takes.
    """
    for token in tokens:
        encoding = keys.encode(token)
        if encoding.kind == "backtab":
            args = ("send-keys", "-t", SESSION, str(encoding.value))
        elif encoding.kind == "csi-u":
            args = ("send-keys", "-t", SESSION, "-H", *encoding.value)
        elif encoding.kind == "tmux":
            args = ("send-keys", "-t", SESSION, str(encoding.value))
        else:
            args = ("send-keys", "-t", SESSION, "-l", "--", token)
        if os.environ.get("MAGIT_LSP_UI_PROBE") == "1" and token == "Enter":
            with (run.dir / "input-probe.jsonl").open("a") as probe:
                probe.write(json.dumps({"time_ns": time.time_ns(), "token": token,
                                        "tmux_args": args}) + "\n")
        _tmux(run, *args)


def _pane_dead(run: Run) -> bool:
    result = _tmux(
        run, "display-message", "-p", "-t", SESSION, "#{pane_dead}", check=False
    )
    return result.returncode != 0 or result.stdout.strip() == "1"


def _wait_for_socket(
    run: Run,
    timeout_ms: int = SOCKET_TIMEOUT_MS,
    readiness_probe=None,
) -> None:
    readiness_probe = readiness_probe or _idle_probe
    deadline = time.monotonic() + timeout_ms / 1000
    last_probe: startup.IdleProbe | None = None
    while time.monotonic() < deadline:
        if run.socket_path.exists():
            last_probe = readiness_probe(run)
            if last_probe.outcome == "true":
                return
        if _pane_dead(run):
            raise StartupError(
                "Neovim exited before startup readiness was reached:\n"
                f"{startup_report(run, idle_probe=last_probe)}"
            )
        time.sleep(0.025)
    socket = startup.inspect_socket(run.socket_path)
    if socket.exists and socket.is_socket:
        reason = (
            "Neovim RPC socket answered, but idle readiness was not reached "
            f"within {timeout_ms} ms"
        )
    else:
        reason = f"Neovim did not establish RPC/idle readiness within {timeout_ms} ms"
    raise StartupError(
        f"{reason}:\n{startup_report(run, idle_probe=last_probe)}"
    )


def _wait_for_view(run: Run, timeout_ms: int = START_TIMEOUT_MS) -> None:
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        try:
            state = snapshot(run)
        except DriverError:
            state = None
        if (
            state
            and (state.get("ready") if run.client == "plain" else
                 state.get("view_stack") and (state.get("render") or {}).get("version", 0) > 0)
            and state.get("idle")
        ):
            return
        if _pane_dead(run):
            raise StartupError(
                "Neovim exited before the first view rendered:\n"
                f"{startup_report(run)}"
            )
        time.sleep(0.025)
    raise DriverError(f"the status view did not render in time:\n{capture(run)}")


def _cleanup(run: Run) -> None:
    """Remove every resource a run created; safe to call repeatedly."""
    stop(run)


def _launcher_path(run: Run) -> Path:
    """The generated shell script that starts Neovim with its cleared env."""
    return run.dir / "launch-nvim.sh"


def _stderr_path(run: Run) -> Path:
    """Where the launcher sends Neovim's stderr for startup diagnosis."""
    return run.dir / "nvim.stderr"


def _exit_path(run: Run) -> Path:
    """Where the launcher records Neovim's exit status."""
    return run.dir / "nvim.exit"


def _shell() -> str:
    """The POSIX shell used to run the launcher script."""
    return shutil.which("sh") or "/bin/sh"


def _nvim_argv(run: Run, init: Path = MINIMAL_INIT, initial_file: Path | None = None) -> list[str]:
    """The Neovim server command, without the environment assignment list."""
    nvim = shutil.which("nvim") or "nvim"
    return [
        nvim,
        "--listen",
        str(run.socket_path),
        "-u",
        str(init),
        "-i",
        "NONE",
        "-n",
        "--cmd",
        "set ttimeoutlen=0",
        *([str(initial_file)] if initial_file is not None else []),
    ]


def _launcher_script(run: Run, env: dict[str, str], init: Path = MINIMAL_INIT, initial_file: Path | None = None, roles: dict[str, Path] | None = None) -> Path:
    """Write a tiny launcher script and return its path.

    Passing the cleared environment on the tmux command line made `respawn-pane`
    fail with "command too long" in Nix development shells, whose environment
    is many kilobytes. The script keeps the tmux command short while preserving
    the exact environment and capturing stderr plus the exit status.
    """
    script = _launcher_path(run)
    arguments = [
        "env",
        "-i",
        *[f"{key}={value}" for key, value in sorted(env.items())],
        *_nvim_argv(run, init, initial_file),
    ]
    if roles and "editor" in roles:
        wrapper = _role_wrapper(run, "editor", roles["editor"], _nvim_argv(run, init, initial_file))
        arguments[-len(_nvim_argv(run, init, initial_file)):] = [str(wrapper)]
    lines = [
        "#!/bin/sh",
        f"# Generated by scripts/ui; {len(env)} environment entries.",
        f"{shlex.join(arguments)} 2>{shlex.quote(str(_stderr_path(run)))}",
        "status=$?",
        f"printf '%s\\n' \"$status\" > {shlex.quote(str(_exit_path(run)))}",
        'exit "$status"',
        "",
    ]
    script.write_text("\n".join(lines), encoding="utf-8")
    # The script contains the whole cleared environment, which may include
    # credentials inherited from the parent shell; keep it owner-only.
    script.chmod(0o600)
    return script


def _role_wrapper(run: Run, role: str, pid_file: Path, command: list[str]) -> Path:
    """Write an exec-safe wrapper that atomically records one process role."""
    wrapper = run.dir / f"{role}-exec.sh"
    from .process_identity import record_command
    wrapper.write_text(
        "#!/bin/sh\n"
        "set -e\n"
        + record_command(pid_file)
        + f"exec {shlex.join(command)}\n",
        encoding="utf-8",
    )
    wrapper.chmod(0o700)
    return wrapper


def _launcher_command(run: Run) -> str:
    """The short tmux command that runs the generated launcher script."""
    return shlex.join([_shell(), str(_launcher_path(run))])


def startup_report(
    run: Run, *, idle_probe: startup.IdleProbe | None = None
) -> str:
    """Describe why Neovim or its RPC socket did not come up.

    Observations are collected independently: socket stat, pane liveness,
    recorded exit status, captured stderr and one bounded connect attempt. This
    is a startup/socket report, not a scenario expectation failure.
    """
    socket = startup.inspect_socket(run.socket_path)
    pane_dead = _pane_dead(run)
    exit_status = startup.read_exit_status(_exit_path(run))
    stderr = startup.read_tail(_stderr_path(run))
    if socket.exists and socket.is_socket:
        connect = startup.connect_probe(run.socket_path)
    else:
        connect = startup.ConnectState(
            attempted=False, ok=False, detail="no socket to contact"
        )
    launcher = _launcher_path(run)
    tmux_command_bytes = (
        len(os.fsencode(_launcher_command(run))) if launcher.is_file() else None
    )
    return startup.format_report(
        socket=socket,
        connect=connect,
        pane_dead=pane_dead,
        exit_status=exit_status,
        stderr=stderr,
        tmux_command_bytes=tmux_command_bytes,
        idle_probe=idle_probe,
        client_state=_client_startup_state(run) if idle_probe is not None else None,
    )


def start(
    fixture: str,
    size: str = f"{DEFAULT_SIZE[0]}x{DEFAULT_SIZE[1]}",
    profile: str = DEFAULT_PROFILE,
    color: str = "ansi",
    extra_env: dict[str, str] | None = None,
    client: str = "rich",
) -> Run:
    """Build a fixture, start Neovim in tmux and wait for the first render.

    `extra_env` adds scenario-specific variables (such as
    `MAGIT_LSP_PLACEMENT`) to Neovim's otherwise cleared environment.
    `client` selects the Rich adapter or the plugin-free Plain initialization.
    """
    check_tools()
    if client not in ("rich", "plain"):
        raise DriverError(f"unknown client `{client}`; expected rich or plain")
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
        socket=str(run_socket_path(run_id)),
        trace=str(directory / "trace.jsonl"),
        tmux=f"magit-ui-{run_id}",
        client=client,
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
                "MAGIT_LSP_BIN": str(Path(__file__).resolve().parents[2] / "target" / "debug" / "magit-lsp"),
                "MAGIT_LSP_TRACE": str(run.trace_path),
                "MAGIT_LSP_UI_ROOT": str(run.repo_path),
            }
        )
        if extra_env:
            env.update(extra_env)

        socket = startup.inspect_socket(run.socket_path)
        if socket.too_long:
            raise StartupError(
                "cannot start Neovim: the RPC socket path is too long:\n"
                + startup.format_report(
                    socket=socket,
                    connect=startup.ConnectState(
                        attempted=False,
                        ok=False,
                        detail="not attempted: socket path too long",
                    ),
                    pane_dead=None,
                    exit_status=None,
                    stderr=[],
                )
            )
        if client == "plain":
            initial_file = run.repo_path / ".magit"
            initial_file.touch()
            with (run.repo_path / ".git/info/exclude").open("a", encoding="utf-8") as exclude:
                exclude.write("\n/.magit\n")
            env.update({"PLAIN_REPO": str(run.repo_path), "PLAIN_SERVER": env["MAGIT_LSP_BIN"],
                        "MAGIT_LSP_UI_CLIENT": "plain", "XDG_STATE_HOME": str(run.home_path / "state")})
            if os.environ.get("MAGIT_LSP_UI_PROBE") == "1":
                env["MAGIT_LSP_UI_PROBE_FILE"] = str(run.dir / "native-input-probe.jsonl")
            roles = {role: run.dir / f"{role}.pid" for role in ("editor", "server")}
            wrapper = _role_wrapper(run, "server", roles["server"], [env["PLAIN_SERVER"]])
            env["MAGIT_LSP_SERVER_WRAPPER"] = str(wrapper)
            _launcher_script(run, env, PLAIN_INIT, initial_file, roles)
        else:
            _launcher_script(run, env)

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
            _launcher_command(run),
        )

        _wait_for_socket(run)
        _wait_for_view(run)
        _update_last_run(run.id)
        return run
    except BaseException:
        try:
            _cleanup(run)
        except BaseException:
            pass
        raise


@contextlib.contextmanager
def native_session(
    repo: Path,
    home: Path,
    init: Path,
    initial_file: Path,
    env: dict[str, str],
    size: str = f"{DEFAULT_SIZE[0]}x{DEFAULT_SIZE[1]}",
    ownership: dict[str, int | None] | None = None,
    server_command: list[str] | None = None,
):
    """Run Neovim for an externally prepared repo and always clean up its session.

    `env` is the complete environment passed to Neovim. Socket and launcher state
    live in a short temporary path, independent of repository and home paths.
    When given, `server_command` is wrapped and its path is exposed as
    `MAGIT_LSP_SERVER_WRAPPER` in the Neovim environment.
    """
    cols, rows = parse_size(size)
    check_tools()
    directory = Path(tempfile.mkdtemp(prefix="magit-ui-"))
    run_id = new_run_id()
    run = Run(run_id, "external", "native", cols, rows, "ansi",
              str(directory), str(repo), str(home), str(directory / "nvim.sock"),
              str(directory / "trace.jsonl"), f"magit-ui-{run_id}")
    try:
        run.trace_path.touch()
        socket = startup.inspect_socket(run.socket_path)
        if socket.too_long:
            raise StartupError("native session socket path is too long")
        role_files = {role: directory / f"{role}.pid" for role in ("editor", "server")}
        if server_command is not None:
            server_wrapper = _role_wrapper(run, "server", role_files["server"], server_command)
            env = {**env, "MAGIT_LSP_SERVER_WRAPPER": str(server_wrapper)}
        _launcher_script(run, env, init, initial_file, role_files)
        _tmux(run, "new-session", "-d", "-s", SESSION, "-x", str(run.cols), "-y", str(run.rows), "-c", str(repo))
        _tmux(run, "set", "-g", "escape-time", "0")
        _tmux(run, "set", "-g", "status", "off")
        _tmux(run, "set", "-g", "default-terminal", "xterm-256color")
        _tmux(run, "respawn-pane", "-k", "-t", SESSION, "-c", str(repo), _launcher_command(run))
        _wait_for_socket(run, readiness_probe=_rpc_probe)
        yield run
    finally:
        body_failed = sys.exc_info()[0] is not None
        if ownership is not None:
            for role in ("editor", "server"):
                path = directory / f"{role}.pid"
                try:
                    ownership[role] = int(path.read_text(encoding="utf-8").strip())
                except (OSError, ValueError):
                    ownership[role] = None
        try:
            report = stop(run)
        except BaseException:
            if not body_failed:
                raise


def _stop_roles(run: Run) -> dict:
    """Bound teardown of recorded process roles and report any surviving owner."""
    from .process_identity import signal_owned
    ownership = {}
    for role in ("editor", "server"):
        path = run.dir / f"{role}.pid"
        if path.exists():
            ownership[role] = int(path.read_text(encoding="utf-8").strip())

    def running(pid: int) -> bool:
        try:
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            return False

    remaining = []
    # Stop the child first, while the editor can still reap it. Never signal a
    # process discovered by name: ownership comes only from the exec wrappers.
    for role in ("server", "editor"):
        pid = ownership.get(role)
        if pid is None:
            continue
        for sig in (signal.SIGTERM, signal.SIGKILL):
            if not running(pid):
                break
            try:
                if not signal_owned(pid, run.dir / f"{role}.pid", sig):
                    break
            except ProcessLookupError:
                break
            deadline = time.monotonic() + 2.0
            while running(pid) and time.monotonic() < deadline:
                time.sleep(0.02)
        if running(pid):
            remaining.append(pid)
    return {"ownership": ownership, "remaining_owned_pids": remaining}


def stop(run: Run) -> dict:
    """Attempt bounded Neovim and tmux teardown, then delete run files."""
    role_file = run.dir / "server.pid"
    command = ":qa!"
    if role_file.is_file():
        pid = int(role_file.read_text(encoding="utf-8").strip())
        script = json.dumps(str(NATIVE_SHUTDOWN), ensure_ascii=False)
        command = f":lua dofile({script})({pid})"
    if run.socket_path.exists():
        try:
            _nvim(
                run,
                "--remote-send",
                f"<C-\\><C-N>{command}<CR>",
                timeout=3.0,
            )
        except (OSError, subprocess.TimeoutExpired):
            pass
        deadline = time.monotonic() + (4.0 if role_file.is_file() else 2.0)
        while run.socket_path.exists() and time.monotonic() < deadline:
            time.sleep(0.02)
    report = _stop_roles(run)
    try:
        _tmux(run, "kill-server", check=False)
    except (OSError, subprocess.TimeoutExpired):
        pass
    report["socket_remains"] = run.socket_path.exists()
    try:
        report["tmux_session_remains"] = _tmux(run, "has-session", "-t", SESSION, check=False).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        report["tmux_session_remains"] = None
    report["cleanup_verified"] = (
        set(report["ownership"]) == {"editor", "server"}
        and not report["remaining_owned_pids"]
        and not report["socket_remains"]
        and report["tmux_session_remains"] is False
    )
    if not report["remaining_owned_pids"]:
        if os.environ.get("MAGIT_LSP_UI_PROBE") == "1":
            probe_dir = BUILD_UI / "probes"
            probe_dir.mkdir(parents=True, exist_ok=True)
            for name in ("input-probe.jsonl", "native-input-probe.jsonl"):
                source = run.dir / name
                if source.is_file():
                    shutil.copy2(source, probe_dir / f"{run.id}-{name}")
        shutil.rmtree(run.dir, ignore_errors=True)
        _forget_last_run(run.id)
    return report


def _read_events(path: Path) -> list[dict]:
    """Read one JSON-lines trace file, ignoring a partial last line."""
    events: list[dict] = []
    if not path.is_file():
        return events
    for line in path.read_text(encoding="utf-8").splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            continue
    return events


def read_trace(run: Run) -> list[dict]:
    """Read and merge the client and server trace files (§A.4).

    The two processes each keep their own monotonic clock, so the merge uses
    the wall-clock `t_unix_ms` both now emit. Files without it (older fixtures)
    are concatenated in read order.
    """
    client = _read_events(run.trace_path)
    server = _read_events(Path(f"{run.trace_path}.server"))
    events = client + server
    if events and all("t_unix_ms" in event for event in events):
        events.sort(key=lambda event: event["t_unix_ms"])
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
            when = float(event.get("t_unix_ms", event.get("t_ms", 0.0)))
            delta = 0.0 if previous is None else when - previous
            previous = when
            detail = event.get("detail")
            suffix = "" if detail in (None, {}) else " " + json.dumps(detail)
            source = event.get("source")
            origin = f" [{source}]" if source else ""
            lines.append(
                f"#{event.get('id')} +{delta:8.3f}ms {event.get('event')}{origin}{suffix}"
            )
    return "\n".join(lines)
