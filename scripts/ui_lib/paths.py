"""Fixed repository paths used by the UI harness."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
MINIMAL_INIT = SCRIPTS / "minimal_init.lua"
BUILD_UI = ROOT / "build" / "ui"
RUNS = BUILD_UI / "runs"
LAST_RUN = BUILD_UI / "last_run"


def run_dir(run_id: str) -> Path:
    """Return the state directory for `run_id`."""
    return RUNS / run_id
