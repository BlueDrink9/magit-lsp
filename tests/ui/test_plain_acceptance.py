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


class GitScalarOutputTest(unittest.TestCase):
    """Keep scalar Git normalization separate from file content reads."""

    def test_scalar_output_loses_command_newline(self) -> None:
        for output, expected in (("revision-id\n", "revision-id"), ("1\r\n", "1")):
            with self.subTest(output=output), patch.object(
                runner["subprocess"], "run",
                return_value=subprocess.CompletedProcess([], 0, stdout=output, stderr=""),
            ) as run:
                raw_output = runner["run"](["git", "scalar"]).stdout
                self.assertEqual(runner["normalize_scalar_output"](raw_output), expected)
            run.assert_called_once()

    def test_blob_bytes_without_final_newline_remain_unchanged(self) -> None:
        for blob in ("file bytes\x00without newline", "file bytes\x00with newline\n"):
            with self.subTest(blob=blob), patch.object(
                runner["subprocess"], "run",
                return_value=subprocess.CompletedProcess([], 0, stdout=blob, stderr=""),
            ) as run:
                result = runner["run"](["git", "cat-file", "blob"])
            self.assertEqual(result.stdout, blob)
            run.assert_called_once()


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


class RpcPromptRecoveryTest(unittest.TestCase):
    """Require observed native prompts before retrying a timed-out RPC read."""

    def test_observed_hit_enter_prompt_is_acknowledged_before_retry(self) -> None:
        read = unittest.mock.Mock(side_effect=[subprocess.TimeoutExpired("rpc", 2), "state"])
        capture = unittest.mock.Mock(return_value="message\nPress ENTER or type command to continue")
        acknowledge = unittest.mock.Mock()
        evidence: list[dict[str, str]] = []

        result = runner["rpc_with_prompt_recovery"](read, capture, acknowledge, evidence)

        self.assertEqual(result, "state")
        acknowledge.assert_called_once_with("hit-enter")
        self.assertEqual(evidence[0]["prompt"], "hit-enter")

    def test_multiple_observed_prompts_stay_within_retry_bound(self) -> None:
        timeout = subprocess.TimeoutExpired("rpc", 2)
        read = unittest.mock.Mock(side_effect=[timeout, timeout, timeout, "state"])
        capture = unittest.mock.Mock(side_effect=["Press ENTER or type command to continue", "-- More --", "Press ENTER or type command to continue"])
        acknowledge = unittest.mock.Mock()
        evidence: list[dict[str, str]] = []

        result = runner["rpc_with_prompt_recovery"](read, capture, acknowledge, evidence, attempts=4)

        self.assertEqual(result, "state")
        self.assertEqual(read.call_count, 4)
        self.assertEqual(acknowledge.call_args_list, [unittest.mock.call("hit-enter"), unittest.mock.call("more"), unittest.mock.call("hit-enter")])
        self.assertEqual(len(evidence), 3)

    def test_timeout_without_observed_prompt_remains_failure(self) -> None:
        read = unittest.mock.Mock(side_effect=subprocess.TimeoutExpired("rpc", 2))
        capture = unittest.mock.Mock(return_value="ordinary editor screen")
        acknowledge = unittest.mock.Mock()
        evidence: list[dict[str, str]] = []

        with self.assertRaises(subprocess.TimeoutExpired):
            runner["rpc_with_prompt_recovery"](read, capture, acknowledge, evidence)

        read.assert_called_once()
        acknowledge.assert_not_called()
        self.assertEqual(evidence[0]["action"], "none")

    def test_non_timeout_rpc_error_propagates_without_recovery(self) -> None:
        error = RuntimeError("RPC failed")
        read = unittest.mock.Mock(side_effect=error)
        capture = unittest.mock.Mock()
        acknowledge = unittest.mock.Mock()
        evidence: list[dict[str, str]] = []

        with self.assertRaises(RuntimeError) as raised:
            runner["rpc_with_prompt_recovery"](read, capture, acknowledge, evidence)

        self.assertIs(raised.exception, error)
        read.assert_called_once()
        capture.assert_not_called()
        acknowledge.assert_not_called()
        self.assertEqual(evidence, [])

    def test_persistent_prompts_preserve_final_timeout_without_acknowledging(self) -> None:
        timeouts = [subprocess.TimeoutExpired("rpc", 2) for _ in range(3)]
        read = unittest.mock.Mock(side_effect=timeouts)
        capture = unittest.mock.Mock(return_value="Press ENTER or type command to continue")
        acknowledge = unittest.mock.Mock()
        evidence: list[dict[str, str]] = []

        with self.assertRaises(subprocess.TimeoutExpired) as raised:
            runner["rpc_with_prompt_recovery"](read, capture, acknowledge, evidence, attempts=3)

        self.assertIs(raised.exception, timeouts[-1])
        self.assertEqual(read.call_count, 3)
        self.assertEqual(capture.call_count, 3)
        self.assertEqual(acknowledge.call_args_list, [unittest.mock.call("hit-enter")] * 2)
        self.assertEqual(len(evidence), 3)
        self.assertEqual(evidence[-1]["prompt"], "hit-enter")


if __name__ == "__main__":
    unittest.main()
