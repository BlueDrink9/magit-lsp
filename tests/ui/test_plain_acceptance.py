"""Regression tests for retained Plain Neovim process checks."""

from __future__ import annotations

import os
import runpy
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


if __name__ == "__main__":
    unittest.main()
