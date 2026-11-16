"""Unit tests for the command-line helpers (U0.2)."""

from __future__ import annotations

import io
import os
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import cli, driver, keys  # noqa: E402


class SizeTest(unittest.TestCase):
    def test_parse_size(self) -> None:
        self.assertEqual(driver.parse_size("100x30"), (100, 30))
        self.assertEqual(driver.parse_size("80x24"), (80, 24))

    def test_invalid_sizes(self) -> None:
        for value in ("100", "100x0", "axb", ""):
            with self.subTest(value=value):
                with self.assertRaises(driver.DriverError):
                    driver.parse_size(value)


class KeyNameTest(unittest.TestCase):
    def test_named_keys(self) -> None:
        for token in ("Escape", "BSpace", "Enter", "C-c", "Up", "Tab", "F5"):
            with self.subTest(token=token):
                self.assertTrue(driver.is_key_name(token))

    def test_literal_text(self) -> None:
        for token in ("?", "-", "=", "a", "gg", "hello world", "--all"):
            with self.subTest(token=token):
                self.assertFalse(driver.is_key_name(token))


class ArgumentTest(unittest.TestCase):
    def test_take_run(self) -> None:
        self.assertEqual(cli._take_run(["?"]), (None, ["?"]))
        self.assertEqual(cli._take_run(["?", "--run", "abc"]), ("abc", ["?"]))
        self.assertEqual(cli._take_run(["--run=abc", "?"]), ("abc", ["?"]))

    def test_take_value(self) -> None:
        self.assertEqual(cli._take_value(["--ms", "50", "x"], "--ms", "2000"), ("50", ["x"]))
        self.assertEqual(cli._take_value(["--ms=50"], "--ms", "2000"), ("50", []))


class TraceFormatTest(unittest.TestCase):
    def test_per_hop_deltas_and_count(self) -> None:
        events = [
            {"id": 1, "t_ms": 10.0, "event": "key_pressed", "detail": {"steps": ["?"]}},
            {"id": 1, "t_ms": 12.5, "event": "key_sent", "detail": {"key": "?"}},
            {"id": 2, "t_ms": 20.0, "event": "key_pressed", "detail": {"steps": ["j"]}},
        ]
        text = driver.format_trace(events, 2)
        self.assertIn("#1 +   0.000ms key_pressed", text)
        self.assertIn("#1 +   2.500ms key_sent", text)
        self.assertIn("#2 +   0.000ms key_pressed", text)

        only_last = driver.format_trace(events, 1)
        self.assertNotIn("#1", only_last)
        self.assertIn("#2", only_last)
        self.assertEqual(driver.format_trace(events, 0), "")


class KeyCommandTest(unittest.TestCase):
    def test_list_prints_the_encoding_table_without_a_run(self) -> None:
        out = io.StringIO()
        with redirect_stdout(out):
            self.assertEqual(cli.main(["keys", "--list"]), 0)
        text = out.getvalue()
        self.assertIn("NEOVIM SPELLINGS", text)
        self.assertIn("VSCODE", text)
        self.assertIn("BSpace", text)
        self.assertIn("backspace", text)
        self.assertIn("shift+Tab", text)
        self.assertIn("shift+tab", text)
        for key in keys.NAMED_KEYS:
            with self.subTest(key=key.name):
                self.assertIn(key.vscode, text)
                for token in key.tokens:
                    self.assertIn(token, text)
                    self.assertEqual(keys.encode(token).value, key.tmux)

    def test_check_accepts_supported_tokens(self) -> None:
        out = io.StringIO()
        with redirect_stdout(out):
            self.assertEqual(cli.main(["keys", "--check", "C-c", "M-3", "g"]), 0)
        text = out.getvalue()
        self.assertIn("ok 'C-c': tmux key C-c", text)
        self.assertIn("ok 'g': literal text 'g'", text)

    def test_check_rejects_an_unknown_spelling_and_suggests(self) -> None:
        for token, suggestion in (
            ("Escpae", "Escape"), ("ctrl+c", "C-c"), ("F13", "F1"),
            ("HyperKey", "nearest spelling"), ("hello world", "keys <text>"),
            ("C-Delete", "no terminal encoding"),
            ("S-S-Enter", "no terminal encoding"),
        ):
            with self.subTest(token=token):
                error = io.StringIO()
                with redirect_stderr(error):
                    self.assertEqual(cli.main(["keys", "--check", token]), 1)
                self.assertIn(suggestion, error.getvalue())

    def test_check_needs_a_token(self) -> None:
        error = io.StringIO()
        with redirect_stderr(error):
            self.assertEqual(cli.main(["keys", "--check"]), 1)
        self.assertIn("needs at least one key", error.getvalue())

    def test_list_rejects_a_run(self) -> None:
        error = io.StringIO()
        with redirect_stderr(error):
            self.assertEqual(cli.main(["keys", "--list", "--run", "abc"]), 1)
        self.assertIn("does not use a run", error.getvalue())


class WaitCommandTest(unittest.TestCase):
    def test_parses_and_forwards_all_barriers(self) -> None:
        run = object()
        with (
            patch.object(driver, "load_run", return_value=run),
            patch.object(driver, "wait_barriers", return_value=[]) as wait,
        ):
            self.assertEqual(
                cli._wait(
                    [
                        "--text",
                        "main",
                        "--cursor",
                        "1:2",
                        "--view",
                        "status",
                        "--ms",
                        "50",
                    ]
                ),
                0,
            )
        wait.assert_called_once_with(
            run, text="main", cursor=(1, 2), view="status", timeout_ms=50
        )

    def test_without_barriers_waits_for_idle(self) -> None:
        run = object()
        with (
            patch.object(driver, "load_run", return_value=run),
            patch.object(driver, "wait_idle", return_value=True) as idle,
        ):
            self.assertEqual(cli._wait(["--ms", "10"]), 0)
        idle.assert_called_once_with(run, timeout_ms=10)

    def test_timeout_reports_state_screen_and_log_path(self) -> None:
        run = driver.Run(
            id="test-run",
            fixture="clean",
            profile="vim",
            cols=100,
            rows=30,
            color="ansi",
            directory="/tmp/test-run",
            repo="/tmp/test-run/repo",
            home="/tmp/test-run/home",
            socket="/tmp/test-run/nvim.sock",
            trace="/tmp/test-run/trace.jsonl",
            tmux="magit-ui-test-run",
        )
        error = io.StringIO()
        with (
            patch.object(driver, "load_run", return_value=run),
            patch.object(
                driver, "wait_barriers", return_value=["screen text 'prompt'"]
            ),
            patch.object(driver, "state_json", return_value='{"idle":false}'),
            patch.object(driver, "capture", return_value="line one\nline two"),
            redirect_stderr(error),
        ):
            self.assertEqual(cli._wait(["--text", "prompt", "--ms", "5"]), 2)
        text = error.getvalue()
        self.assertIn("timed out after 5 ms waiting for screen text 'prompt'", text)
        self.assertIn('state: {"idle":false}', text)
        self.assertIn("screen excerpt:", text)
        self.assertIn("line one", text)
        self.assertIn("log: /tmp/test-run/trace.jsonl", text)
        self.assertIn("run: /tmp/test-run", text)

    def test_invalid_cursor_is_rejected_without_a_run(self) -> None:
        error = io.StringIO()
        with redirect_stderr(error):
            self.assertEqual(cli.main(["wait", "--cursor", "1:x"]), 1)
        self.assertIn("invalid cursor", error.getvalue())


class MainTest(unittest.TestCase):
    def test_no_arguments_shows_usage(self) -> None:
        out = io.StringIO()
        with redirect_stdout(out):
            self.assertEqual(cli.main([]), 2)
        self.assertIn("scripts/ui start", out.getvalue())

    def test_help_succeeds(self) -> None:
        out = io.StringIO()
        with redirect_stdout(out):
            self.assertEqual(cli.main(["--help"]), 0)
        self.assertIn("scripts/ui keys", out.getvalue())

    def test_unknown_command(self) -> None:
        self.assertEqual(cli.main(["nope"]), 2)

    def test_run_verbose_flags_and_environment_forward(self) -> None:
        for args, env, expected in ((["-v", "case.scn"], {}, True),
                                    (["--verbose", "case.scn"], {}, True),
                                    (["case.scn"], {"UI_VERBOSE": "1"}, True),
                                    (["case.scn"], {}, False)):
            with (
                self.subTest(args=args, env=env),
                patch.dict(os.environ, env, clear=False),
                patch.object(cli.scenario, "run", return_value=0) as run,
            ):
                self.assertEqual(cli._run(args), 0)
                self.assertEqual(run.call_args.kwargs["verbose"], expected)


if __name__ == "__main__":
    unittest.main()
