"""Fixed repository paths used by the UI harness."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
MINIMAL_INIT = SCRIPTS / "minimal_init.lua"
PLAIN_INIT = ROOT / "tests" / "ui" / "plain_init.lua"
NATIVE_SHUTDOWN = SCRIPTS / "ui_shutdown.lua"
BUILD_UI = ROOT / "build" / "ui"
RUNS = BUILD_UI / "runs"
LAST_RUN = BUILD_UI / "last_run"


def run_dir(run_id: str) -> Path:
    """Return the state directory for `run_id`."""
    return RUNS / run_id


def run_socket_path(run_id: str) -> Path:
    """Return a unique RPC socket path outside the potentially deep run directory."""
    return BUILD_UI / f"n-{run_id[-14:]}.sock"
