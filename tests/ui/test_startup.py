"""Unit tests for the startup/socket diagnostics (TASKS §11.3)."""

from __future__ import annotations

import os
import socket
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import startup  # noqa: E402


class InspectSocketTest(unittest.TestCase):
    def test_missing_socket_is_reported_with_its_byte_length(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nvim.sock"
            state = startup.inspect_socket(path)
        self.assertFalse(state.exists)
        self.assertFalse(state.is_socket)
        self.assertIsNone(state.error)
        self.assertEqual(state.length, len(os.fsencode(str(path))))
        self.assertFalse(state.too_long)
        self.assertIn("missing", state.summary())

    def test_live_socket_reports_its_mode(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nvim.sock"
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            try:
                server.bind(str(path))
                state = startup.inspect_socket(path)
            finally:
                server.close()
        self.assertTrue(state.exists)
        self.assertTrue(state.is_socket)
        self.assertIsNotNone(state.mode)
        self.assertIn("is a socket", state.summary())

    def test_regular_file_is_not_a_socket(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nvim.sock"
            path.write_text("not a socket", encoding="utf-8")
            state = startup.inspect_socket(path)
        self.assertTrue(state.exists)
        self.assertFalse(state.is_socket)
        self.assertIn("not a socket", state.summary())

    def test_overlong_path_is_flagged_without_stat_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / ("x" * 130)
            state = startup.inspect_socket(path)
        self.assertTrue(state.too_long)
        self.assertIn("TOO LONG", state.summary())
        self.assertGreater(state.length, startup.SOCKET_PATH_LIMIT)

    def test_length_counts_bytes_not_characters(self) -> None:
        path = Path("/tmp") / ("é" * 20)
        state = startup.inspect_socket(path)
        self.assertEqual(state.length, len(os.fsencode(str(path))))
        self.assertGreater(state.length, len(str(path)))


class ReadTailTest(unittest.TestCase):
    def test_missing_file_is_empty(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(
                startup.read_tail(Path(directory) / "missing.log"), []
            )

    def test_returns_the_last_non_empty_lines(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nvim.stderr"
            path.write_text("one\n\ntwo\nthree\nfour\n", encoding="utf-8")
            self.assertEqual(startup.read_tail(path, lines=2), ["three", "four"])

    def test_zero_lines_returns_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nvim.stderr"
            path.write_text("one\n", encoding="utf-8")
            self.assertEqual(startup.read_tail(path, lines=0), [])


class ReadExitStatusTest(unittest.TestCase):
    def test_reads_an_integer(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nvim.exit"
            path.write_text("7\n", encoding="utf-8")
            self.assertEqual(startup.read_exit_status(path), 7)

    def test_missing_file_is_none(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            self.assertIsNone(
                startup.read_exit_status(Path(directory) / "missing")
            )

    def test_invalid_content_is_none(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nvim.exit"
            path.write_text("not a number", encoding="utf-8")
            self.assertIsNone(startup.read_exit_status(path))


class ConnectProbeTest(unittest.TestCase):
    def test_successful_probe(self) -> None:
        result = subprocess.CompletedProcess([], 0, stdout="1\n", stderr="")
        with patch.object(startup.subprocess, "run", return_value=result) as run:
            state = startup.connect_probe(Path("/tmp/sock"))
        command = run.call_args.args[0]
        self.assertIn("--headless", command)
        self.assertIs(run.call_args.kwargs["stdin"], subprocess.DEVNULL)
        self.assertTrue(state.attempted)
        self.assertTrue(state.ok)
        self.assertIn("connected", state.summary())

    def test_failure_uses_stderr(self) -> None:
        result = subprocess.CompletedProcess(
            [], 1, stdout="", stderr="E247: no server\n"
        )
        with patch.object(startup.subprocess, "run", return_value=result):
            state = startup.connect_probe(Path("/tmp/sock"))
        self.assertTrue(state.attempted)
        self.assertFalse(state.ok)
        self.assertEqual(state.detail, "E247: no server")
        self.assertIn("failed", state.summary())

    def test_failure_without_stderr_reports_the_exit_status(self) -> None:
        result = subprocess.CompletedProcess([], 2, stdout="", stderr="")
        with patch.object(startup.subprocess, "run", return_value=result):
            state = startup.connect_probe(Path("/tmp/sock"))
        self.assertEqual(state.detail, "exit status 2")

    def test_timeout_is_bounded(self) -> None:
        timeout = subprocess.TimeoutExpired([], 2.0)
        with patch.object(startup.subprocess, "run", side_effect=timeout):
            state = startup.connect_probe(Path("/tmp/sock"), timeout=2.0)
        self.assertFalse(state.ok)
        self.assertIn("timed out after 2s", state.detail)

    def test_missing_binary_is_reported(self) -> None:
        with patch.object(startup.subprocess, "run", side_effect=FileNotFoundError):
            state = startup.connect_probe(Path("/tmp/sock"))
        self.assertTrue(state.attempted)
        self.assertFalse(state.ok)


class FormatReportTest(unittest.TestCase):
    def test_report_names_every_observation(self) -> None:
        report = startup.format_report(
            socket=startup.SocketState(
                path="/tmp/nvim.sock",
                length=14,
                exists=True,
                is_socket=True,
                mode="0o600",
            ),
            connect=startup.ConnectState(True, False, "E247: no server"),
            pane_dead=True,
            exit_status=1,
            stderr=["boom", "detail"],
            tmux_command_bytes=42,
        )
        self.assertIn("socket: /tmp/nvim.sock", report)
        self.assertIn("is a socket; mode 0o600", report)
        self.assertIn("nvim pane: exited", report)
        self.assertIn("nvim exit status: 1", report)
        self.assertIn("tmux launcher command: 42 bytes", report)
        self.assertIn("connect probe: failed: E247: no server", report)
        self.assertIn("  boom", report)
        self.assertIn("  detail", report)

    def test_report_distinguishes_rpc_connection_from_idle_readiness(self) -> None:
        report = startup.format_report(
            socket=startup.SocketState(
                path="/tmp/nvim.sock",
                length=14,
                exists=True,
                is_socket=True,
                mode="0o600",
            ),
            connect=startup.ConnectState(True, True, ""),
            pane_dead=False,
            exit_status=None,
            stderr=[],
            idle_probe=startup.IdleProbe("false", 0, "false"),
            client_state=[
                "client work: requests=0, queued=1, prompts=0, scheduled=0, progress=0",
                "views: status (status): version=2, acked=1",
                "recent Neovim messages:",
                "  startup warning",
            ],
        )
        self.assertIn("connect probe: connected", report)
        self.assertIn("idle readiness: returned false", report)
        self.assertIn("stdout: false", report)
        self.assertIn("queued=1", report)
        self.assertIn("version=2, acked=1", report)
        self.assertIn("startup warning", report)
        self.assertNotIn("{", report)

    def test_report_includes_bounded_idle_error_output(self) -> None:
        report = startup.format_report(
            socket=startup.SocketState(path="/tmp/nvim.sock", length=14),
            connect=startup.ConnectState(True, True, ""),
            pane_dead=False,
            exit_status=None,
            stderr=[],
            idle_probe=startup.IdleProbe(
                "error", 2, "", "Lua: E5108: debug unavailable" + "x" * 800
            ),
        )
        self.assertIn("remote-expression error (exit code 2)", report)
        self.assertIn("stderr: Lua: E5108: debug unavailable", report)
        self.assertLess(len(report), 700)

    def test_report_identifies_unexpected_idle_output_and_timeout(self) -> None:
        for probe, expected in (
            (startup.IdleProbe("unexpected", 0, "maybe"), "unexpected output"),
            (startup.IdleProbe("timeout", detail="after 2s"), "timed out: after 2s"),
        ):
            with self.subTest(expected=expected):
                report = startup.format_report(
                    socket=startup.SocketState(path="/tmp/nvim.sock", length=14),
                    connect=startup.ConnectState(True, True, ""),
                    pane_dead=False,
                    exit_status=None,
                    stderr=[],
                    idle_probe=probe,
                )
                self.assertIn(expected, report)

    def test_report_without_socket_or_stderr(self) -> None:
        report = startup.format_report(
            socket=startup.SocketState(path="/tmp/nvim.sock", length=14),
            connect=startup.ConnectState(
                False, False, "not attempted: no socket to contact"
            ),
            pane_dead=None,
            exit_status=None,
            stderr=[],
        )
        self.assertIn("missing", report)
        self.assertIn("nvim pane: unknown", report)
        self.assertIn("nvim exit status: not recorded", report)
        self.assertIn("not attempted", report)
        self.assertIn("nvim stderr (tail): empty", report)


if __name__ == "__main__":
    unittest.main()
