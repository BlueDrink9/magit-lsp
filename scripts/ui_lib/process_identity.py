"""Record and validate process birth identity before signaling native owners."""

from __future__ import annotations

import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys


def identity(pid: int) -> str | None:
    """Read Linux birth ticks, or a portable ps birth timestamp, for a live PID."""
    if pid <= 0:
        return None
    try:
        if sys.platform.startswith("linux"):
            fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
            return f"linux:{fields[19]}"
        result = subprocess.run(
            ["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True,
            text=True, timeout=2, check=False,
        )
        return f"ps:{result.stdout.strip()}" if result.returncode == 0 and result.stdout.strip() else None
    except (OSError, subprocess.TimeoutExpired):
        return None


def matches(pid: int, record: Path) -> bool:
    """Reject missing records and PID reuse rather than guessing ownership."""
    try:
        stored = json.loads(record.with_suffix(".identity").read_text())
    except (OSError, ValueError):
        return False
    current = identity(pid)
    return current is not None and stored == {"pid": pid, "identity": current}


def record_command(record: Path) -> str:
    """Return a shell command recording the wrapper's identity before exec."""
    return shlex.join([sys.executable, str(Path(__file__).resolve()), str(record)]) + ' "$$"\n'


def signal_owned(pid: int, record: Path, sig: int) -> bool:
    """Signal only the recorded birth; Linux pidfds also close the reuse race."""
    if hasattr(os, "pidfd_open") and hasattr(signal, "pidfd_send_signal"):
        descriptor = os.pidfd_open(pid)
        try:
            if not matches(pid, record):
                return False
            signal.pidfd_send_signal(descriptor, sig)
        finally:
            os.close(descriptor)
        return True
    if not matches(pid, record):
        return False
    os.kill(pid, sig)
    return True


if __name__ == "__main__":
    record, pid = Path(sys.argv[1]), int(sys.argv[2])
    birth = identity(pid)
    if birth is None:
        raise RuntimeError("cannot record native process identity")
    record.with_suffix(".identity").write_text(json.dumps({"pid": pid, "identity": birth}))
    temporary = record.with_suffix(".tmp")
    temporary.write_text(f"{pid}\n")
    os.replace(temporary, record)
