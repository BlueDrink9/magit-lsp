#!/usr/bin/env python3
"""Launch an isolated manual Helix checklist; never infer checklist acceptance."""

from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import signal
import subprocess
import time
from pathlib import Path

from ui_lib import fixtures
from ui_lib.process_identity import record_command, signal_owned

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs/client-helix-reference.md"
CONFIG_MARKER = "<!-- helix-reference-config -->"


def documented_toml(path: Path = DOCS) -> str:
    """Return the fenced TOML block that the Helix reference presents to users."""
    lines = path.read_text(encoding="utf-8").splitlines()
    try:
        marker = lines.index(CONFIG_MARKER)
    except ValueError as error:
        raise RuntimeError(f"documented Helix config marker missing from {path}") from error
    start = next((index for index in range(marker + 1, len(lines)) if lines[index].strip() == "```toml"), None)
    if start is None:
        raise RuntimeError(f"no TOML fence follows the Helix config marker in {path}")
    end = next((index for index in range(start + 1, len(lines)) if lines[index].strip() == "```"), None)
    if end is None:
        raise RuntimeError(f"unterminated TOML fence after the Helix config marker in {path}")
    return "\n".join(lines[start + 1:end]) + "\n"


def run(*args: str, **kwargs: object) -> str:
    """Run one bounded setup or tmux command and retain actionable failures."""
    result = subprocess.run(args, capture_output=True, text=True, timeout=15, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{shlex.join(args)}: {result.stderr.strip()}")
    return result.stdout


def executing(pid: int) -> bool:
    """Distinguish an executing owned process from a Linux defunct entry."""
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    status = Path(f"/proc/{pid}/status")
    return not (status.is_file() and "\nState:\tZ" in status.read_text())


def main() -> None:
    """Prepare or stop a manual native session without an editor test adapter."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--helix", default="hx")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/magit-lsp")
    parser.add_argument("--directory", type=Path, default=ROOT / "build/ui/helix-smoke")
    parser.add_argument("--stop", action="store_true")
    args = parser.parse_args()
    directory = args.directory.resolve()
    state_path = directory / "session.json"
    if args.stop:
        state = json.loads(state_path.read_text())
        socket = state["tmux"]
        subprocess.run(["tmux", "-L", socket, "send-keys", "-t", "helix", "Escape", ":lsp-stop", "Enter"], timeout=5)
        pid_file = directory / "server.pid"
        if pid_file.exists():
            pid = int(pid_file.read_text())
            for sig in (None, signal.SIGTERM, signal.SIGKILL):
                if sig is not None:
                    try:
                        if not signal_owned(pid, pid_file, sig):
                            raise RuntimeError(f"refusing to signal mismatched process identity: {pid}")
                    except ProcessLookupError:
                        break
                deadline = time.monotonic() + 2
                while time.monotonic() < deadline:
                    if not executing(pid):
                        break
                    time.sleep(0.02)
                else:
                    continue
                break
            else:
                raise RuntimeError(f"owned Helix server {pid} was not reaped")
        subprocess.run(["tmux", "-L", socket, "kill-server"], timeout=5)
        (directory / "cleanup.json").write_text(json.dumps({
            "server_pid": int(pid_file.read_text()) if pid_file.exists() else None,
            "server_executing": executing(int(pid_file.read_text())) if pid_file.exists() else None,
            "server_proc_entry_remains": Path(f"/proc/{int(pid_file.read_text())}").exists() if pid_file.exists() else None,
        }, indent=2) + "\n")
        print("Stopped manual Helix session; fixture and logs retained.")
        return
    if state_path.exists():
        raise RuntimeError("session directory already exists; select a fresh --directory")
    helix = shutil.which(args.helix)
    if not helix or not args.binary.is_file():
        raise RuntimeError("Helix and the freshly built server are required")
    home, repo = directory / "home", directory / "repo"
    fixtures.build("modified-two-files", repo, home)
    (repo / ".git/info/exclude").write_text("/.magit\n")
    (repo / ".magit").write_text("")
    config = home / "config/helix"
    config.mkdir(parents=True)
    (config / "languages.toml").write_text(documented_toml())
    tools = directory / "bin"
    tools.mkdir()
    server = tools / "magit-lsp"
    server.write_text("#!/bin/sh\nset -e\n" + record_command(directory / "server.pid") + f"exec {shlex.quote(str(args.binary.resolve()))}\n")
    server.chmod(0o700)
    env = fixtures.determinism_env(home)
    env.update(PATH=f"{tools}:{os.environ['PATH']}", TERM="xterm-256color", XDG_CACHE_HOME=str(home / "cache"))
    socket = f"magit-helix-{os.getpid()}"
    launch = directory / "launch.sh"
    launch.write_text("#!/bin/sh\nexec " + shlex.join([
        "env", "-i", *(f"{k}={v}" for k, v in sorted(env.items())), helix,
        "-vv", "--log", str(directory / "helix.log"), ".magit",
    ]) + "\n")
    launch.chmod(0o700)
    run("tmux", "-L", socket, "new-session", "-d", "-s", "helix", "-x", "100", "-y", "30", "-c", str(repo), str(launch))
    state = {"version": run(helix, "--version").strip(), "tmux": socket,
             "repo": str(repo), "helix": helix, "server": str(args.binary.resolve())}
    state_path.write_text(json.dumps(state, indent=2) + "\n")
    print(f"{state['version']}; manual tmux session: {socket}; evidence: {directory}")


if __name__ == "__main__":
    main()
