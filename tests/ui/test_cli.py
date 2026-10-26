"""Unit tests for the command-line helpers (U0.2)."""

from __future__ import annotations

import io
import os
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import cli, driver  # noqa: E402


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
