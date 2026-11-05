"""Regression tests for retained Plain Neovim process checks."""

from __future__ import annotations

import os
import runpy
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
runner = runpy.run_path(str(ROOT / "scripts/test_plain_neovim.py"), run_name="plain_neovim_runner")


class OwnedPidTest(unittest.TestCase):
    """Check portable process existence handling for owned PIDs."""

    def test_running_pid(self) -> None:
        with patch.object(os, "kill") as kill:
            self.assertTrue(runner["pid_is_running"](123))
            kill.assert_called_once_with(123, 0)

    def test_missing_pid(self) -> None:
        with patch.object(os, "kill", side_effect=ProcessLookupError):
            self.assertFalse(runner["pid_is_running"](123))

    def test_inaccessible_pid_is_running(self) -> None:
        with patch.object(os, "kill", side_effect=PermissionError):
            self.assertTrue(runner["pid_is_running"](123))

    def test_cleanup_rejects_missing_owned_role(self) -> None:
        verify = runner["cleanup_verified"]
        self.assertFalse(verify({"editor": 123, "server": None}, [], False, False))
        self.assertTrue(verify({"editor": 123, "server": 456}, [], False, False))


class TmuxCleanupTest(unittest.TestCase):
    """Check teardown probes the actual shared tmux session."""

    def test_live_shared_session_is_reported_present(self) -> None:
        with patch.object(runner["subprocess"], "run", return_value=subprocess.CompletedProcess([], 0)) as run:
            self.assertTrue(runner["tmux_session_remains"]("owned-server"))
        run.assert_called_once_with(
            ["tmux", "-L", "owned-server", "has-session", "-t", "magit"],
            capture_output=True, timeout=5,
        )


if __name__ == "__main__":
    unittest.main()
