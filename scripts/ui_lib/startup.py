"""Startup and RPC-socket diagnostics for the real-UI harness (spec §A.1).

The helpers here are deliberately independent of the driver: they inspect the
filesystem and run one bounded connection probe, and return plain data that the
driver can print when Neovim does not come up. A startup failure must be
reported as a startup/socket outcome, not as a scenario assertion failure.
"""

from __future__ import annotations

import os
import shutil
import stat
import subprocess
from dataclasses import dataclass
from pathlib import Path

# A Linux `sockaddr_un.sun_path` holds 108 bytes including the terminating NUL,
# so the usable path is at most 107 bytes. Neovim fails to bind a longer path.
SOCKET_PATH_LIMIT = 107
STDERR_TAIL_LINES = 20
DIAGNOSTIC_TEXT_LIMIT = 400


@dataclass(frozen=True)
class SocketState:
    """The filesystem state of the expected Neovim RPC socket."""

    path: str
    length: int
    exists: bool = False
    is_socket: bool = False
    mode: str | None = None
    error: str | None = None

    @property
    def too_long(self) -> bool:
        """True when the path exceeds the Unix-socket name limit."""
        return self.length > SOCKET_PATH_LIMIT

    def summary(self) -> str:
        """One-line description used in startup reports."""
        parts = [
            f"{self.path} ({self.length} bytes, "
            f"limit {SOCKET_PATH_LIMIT})"
        ]
        if self.too_long:
            parts.append("TOO LONG")
        if self.error is not None:
            parts.append(f"stat error: {self.error}")
        elif not self.exists:
            parts.append("missing")
        else:
            parts.append("is a socket" if self.is_socket else "not a socket")
            if self.mode is not None:
                parts.append(f"mode {self.mode}")
        return "; ".join(parts)


@dataclass(frozen=True)
class ConnectState:
    """The outcome of one `nvim --server … --remote-expr` probe."""

    attempted: bool
    ok: bool
    detail: str

    def summary(self) -> str:
        """One-line description used in startup reports."""
        if not self.attempted:
            return f"not attempted: {_bounded(self.detail)}"
        return "connected" if self.ok else f"failed: {_bounded(self.detail)}"


@dataclass(frozen=True)
class IdleProbe:
    """The last bounded result of the driver's Neovim idle expression."""

    outcome: str
    returncode: int | None = None
    stdout: str = ""
    stderr: str = ""
    detail: str = ""

    def summary(self) -> str:
        """Describe the idle outcome with bounded stdout/stderr evidence."""
        labels = {
            "true": "returned true",
            "false": "returned false",
            "error": (
                f"remote-expression error (exit code {self.returncode})"
                if self.returncode is not None
                else "remote-expression could not run"
            ),
            "timeout": f"remote-expression timed out{': ' + self.detail if self.detail else ''}",
            "unexpected": "returned unexpected output",
            "unavailable": "not attempted",
        }
        lines = [f"idle readiness: {labels.get(self.outcome, self.outcome)}"]
        if self.stdout:
            lines.append(f"  stdout: {_bounded(self.stdout)}")
        if self.stderr:
            lines.append(f"  stderr: {_bounded(self.stderr)}")
        if self.detail and self.outcome not in ("timeout",):
            lines.append(f"  detail: {_bounded(self.detail)}")
        return "\n".join(lines)


def _bounded(text: str) -> str:
    """Collapse control whitespace and cap an untrusted diagnostic string."""
    value = " ".join(text.split())
    if len(value) > DIAGNOSTIC_TEXT_LIMIT:
        return value[: DIAGNOSTIC_TEXT_LIMIT - 3] + "..."
    return value


def inspect_socket(path: Path) -> SocketState:
    """Stat `path` without following surprises, reporting type and mode.

    The byte length is measured on the encoded pathname because the kernel
    limit applies to bytes, not to decoded characters.
    """
    encoded = os.fsencode(str(path))
    length = len(encoded)
    try:
        info = path.stat()
    except FileNotFoundError:
        return SocketState(path=str(path), length=length)
    except OSError as error:
        return SocketState(path=str(path), length=length, error=str(error))
    return SocketState(
        path=str(path),
        length=length,
        exists=True,
        is_socket=stat.S_ISSOCK(info.st_mode),
        mode=oct(stat.S_IMODE(info.st_mode)),
    )


def read_tail(path: Path, lines: int = STDERR_TAIL_LINES) -> list[str]:
    """Return the last `lines` non-empty lines of a log, or an empty list."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    content = [line for line in text.splitlines() if line.strip()]
    return content[-lines:] if lines > 0 else []


def read_exit_status(path: Path) -> int | None:
    """Read a recorded process exit status, or return None when absent."""
    try:
        text = path.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    try:
        return int(text)
    except ValueError:
        return None


def connect_probe(
    socket: Path, nvim: str | None = None, timeout: float = 2.0
) -> ConnectState:
    """Ask the server for a trivial expression, bounded by `timeout`.

    This is intentionally independent of the driver's `_expr`: a startup report
    must still work when the driver's normal connection path is what failed.
    """
    command = [
        nvim or shutil.which("nvim") or "nvim",
        "--headless",
        "--server",
        str(socket),
    ]
    command += ["--remote-expr", "1"]
    try:
        result = subprocess.run(
            command,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return ConnectState(
            attempted=True, ok=False, detail=f"timed out after {timeout:g}s"
        )
    except OSError as error:
        return ConnectState(attempted=True, ok=False, detail=str(error))
    detail = " ".join((result.stderr or result.stdout).split())
    if result.returncode == 0:
        return ConnectState(attempted=True, ok=True, detail="")
    return ConnectState(
        attempted=True,
        ok=False,
        detail=detail or f"exit status {result.returncode}",
    )


def format_report(
    *,
    socket: SocketState,
    connect: ConnectState,
    pane_dead: bool | None,
    exit_status: int | None,
    stderr: list[str],
    tmux_command_bytes: int | None = None,
    idle_probe: IdleProbe | None = None,
    client_state: list[str] | None = None,
) -> str:
    """Render the startup/socket outcome as a multi-line diagnostic block.

    Every field is optional because a startup can fail before a process, a
    socket or a launcher exists. Keeping each observation on its own line makes
    it obvious which evidence was available.
    """
    lines = [f"socket: {socket.summary()}"]
    if pane_dead is None:
        lines.append("nvim pane: unknown")
    else:
        lines.append(f"nvim pane: {'exited' if pane_dead else 'alive'}")
    if exit_status is None:
        lines.append("nvim exit status: not recorded")
    else:
        lines.append(f"nvim exit status: {exit_status}")
    if tmux_command_bytes is not None:
        lines.append(f"tmux launcher command: {tmux_command_bytes} bytes")
    lines.append(f"connect probe: {connect.summary()}")
    if idle_probe is not None:
        lines.extend(idle_probe.summary().splitlines())
    if client_state:
        lines.extend(client_state)
    if stderr:
        lines.append("nvim stderr (tail):")
        lines.extend(f"  {_bounded(line)}" for line in stderr)
    else:
        lines.append("nvim stderr (tail): empty")
    return "\n".join(lines)
