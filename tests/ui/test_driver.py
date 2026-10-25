"""Unit tests for the tmux/Neovim driver helpers (spec §A.1)."""

from __future__ import annotations

import base64
import json
import re
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import driver, startup  # noqa: E402


def make_run(directory: str) -> driver.Run:
    """Build an unstarted `Run` rooted at `directory`."""
    base = Path(directory)
    return driver.Run(
        id="test-run",
        fixture="clean",
        profile="vim",
        cols=100,
        rows=30,
        color="ansi",
        directory=str(base),
        repo=str(base / "repo"),
        home=str(base / "home"),
        socket=str(base / "nvim.sock"),
        trace=str(base / "trace.jsonl"),
        tmux="magit-ui-test-run",
    )



class DocumentTest(unittest.TestCase):
    def test_returns_the_exact_text(self) -> None:
        text = "  indented\n\nlast line  "
        with patch.object(driver, "_expr", return_value=json.dumps(text)):
            self.assertEqual(driver.document(object()), text)

    def test_empty_document(self) -> None:
        with patch.object(driver, "_expr", return_value=json.dumps("")):
            self.assertEqual(driver.document(object()), "")

    def test_missing_answer_is_an_error(self) -> None:
        with patch.object(driver, "_expr", return_value=None):
            with self.assertRaises(driver.DriverError):
                driver.document(object())

    def test_invalid_json_is_an_error(self) -> None:
        with patch.object(driver, "_expr", return_value="not json"):
            with self.assertRaises(driver.DriverError):
                driver.document(object())

    def test_non_string_answer_is_an_error(self) -> None:
        with patch.object(driver, "_expr", return_value="[]"):
            with self.assertRaises(driver.DriverError):
                driver.document(object())

    def test_expression_reads_the_active_buffer(self) -> None:
        self.assertIn("nvim_buf_get_lines", driver._DOCUMENT_EXPR)
        self.assertIn("vim.json.encode", driver._DOCUMENT_EXPR)


class GotoTest(unittest.TestCase):
    def test_sends_the_literal_text_and_sets_the_cursor(self) -> None:
        with patch.object(driver, "_expr", return_value="1") as expression:
            driver.goto_text(object(), "+line3 CHANGED")
        request = expression.call_args.args[1]
        payload = re.search(r'base64\.decode\("([A-Za-z0-9+/=]*)"\)', request)
        self.assertIsNotNone(payload)
        self.assertEqual(
            base64.b64decode(payload.group(1)).decode("utf-8"), "+line3 CHANGED"
        )
        self.assertIn("line:find(text, 1, true)", request)
        self.assertIn("nvim_win_set_cursor", request)

    def test_true_answer_is_success(self) -> None:
        with patch.object(driver, "_expr", return_value="true"):
            driver.goto_text(object(), "text")

    def test_missing_text_is_an_error(self) -> None:
        with patch.object(driver, "_expr", return_value="0"):
            with self.assertRaises(driver.DriverError):
                driver.goto_text(object(), "absent")

    def test_no_answer_is_an_error(self) -> None:
        with patch.object(driver, "_expr", return_value=None):
            with self.assertRaises(driver.DriverError):
                driver.goto_text(object(), "text")


class WaitScreenTest(unittest.TestCase):
    def test_returns_true_when_the_text_is_visible(self) -> None:
        with patch.object(driver, "capture", return_value="a prompt here"):
            self.assertTrue(driver.wait_screen(object(), "prompt"))

    def test_timeout_returns_false(self) -> None:
        with patch.object(driver, "capture", return_value="nothing"):
            self.assertFalse(driver.wait_screen(object(), "prompt", timeout_ms=0))

    def test_polls_until_the_text_arrives(self) -> None:
        answers = iter(["nothing", "still nothing", "a prompt here"])
        with patch.object(driver, "capture", side_effect=lambda run: next(answers)):
            with patch.object(driver.time, "sleep"):
                self.assertTrue(
                    driver.wait_screen(object(), "prompt", timeout_ms=1000)
                )

    def test_timeout_is_bounded(self) -> None:
        with patch.object(driver, "capture", return_value="nothing"):
            with patch.object(driver.time, "sleep"):
                self.assertFalse(
                    driver.wait_screen(object(), "prompt", timeout_ms=1)
                )


class WaitPromptTest(unittest.TestCase):
    def test_input_and_textlist_prompts_count(self) -> None:
        for kind in ("@", "-"):
            with self.subTest(kind=kind):
                with patch.object(driver, "_expr", return_value=kind):
                    self.assertTrue(driver.prompt_pending(object()))

    def test_normal_mode_is_not_a_prompt(self) -> None:
        for kind in ("", ":", "="):
            with self.subTest(kind=kind):
                with patch.object(driver, "_expr", return_value=kind):
                    self.assertFalse(driver.prompt_pending(object()))

    def test_wait_prompt_polls_until_pending(self) -> None:
        answers = iter(["", "", "-"])

        def answer(run, expression, timeout=5.0):
            return next(answers)

        with patch.object(driver, "_expr", side_effect=answer):
            with patch.object(driver.time, "sleep"):
                self.assertTrue(driver.wait_prompt(object(), timeout_ms=1000))

    def test_wait_prompt_times_out(self) -> None:
        with patch.object(driver, "_expr", return_value=""):
            with patch.object(driver.time, "sleep"):
                self.assertFalse(driver.wait_prompt(object(), timeout_ms=1))


class KeyNameTest(unittest.TestCase):
    def test_tmux_names(self) -> None:
        for token in ("Escape", "Enter", "BSpace", "C-c", "F5", "Tab"):
            with self.subTest(token=token):
                self.assertTrue(driver.is_key_name(token))

    def test_literal_text(self) -> None:
        for token in ("$", "g", "=", "A", "Alice", "2"):
            with self.subTest(token=token):
                self.assertFalse(driver.is_key_name(token))


class CsiUKeyTest(unittest.TestCase):
    def test_ctrl_enter_becomes_csi_u_bytes(self) -> None:
        self.assertEqual(
            driver._csi_u_bytes("C-Enter"),
            ["1b", "5b", "31", "33", "3b", "35", "75"],
        )

    def test_profile_spellings_normalize(self) -> None:
        expected = ["1b", "5b", "31", "33", "3b", "35", "75"]
        for token in ("C-Enter", "ctrl+Enter", "Ctrl+Enter", "control-Enter"):
            with self.subTest(token=token):
                self.assertEqual(driver._csi_u_bytes(token), expected)

    def test_shift_tab_becomes_csi_u_bytes(self) -> None:
        self.assertEqual(
            driver._csi_u_bytes("shift+Tab"), ["1b", "5b", "39", "3b", "32", "75"]
        )

    def test_ctrl_arrow_uses_the_legacy_form(self) -> None:
        self.assertEqual(
            driver._csi_u_bytes("C-Left"), ["1b", "5b", "31", "3b", "35", "44"]
        )

    def test_arrow_aliases(self) -> None:
        self.assertEqual(
            driver._csi_u_bytes("ctrl+ArrowDown"),
            ["1b", "5b", "31", "3b", "35", "42"],
        )

    def test_modified_page_keys_use_tilde_form(self) -> None:
        self.assertEqual(
            driver._csi_u_bytes("C-PPage"), ["1b", "5b", "35", "3b", "35", "7e"]
        )

    def test_alt_stays_on_tmux_named_keys(self) -> None:
        for token in ("M-3", "M-4", "alt+ArrowDown", "alt+x"):
            with self.subTest(token=token):
                self.assertIsNone(driver._csi_u_bytes(token))

    def test_untranslated_tokens_keep_their_existing_path(self) -> None:
        for token in ("Enter", "Tab", "BTab", "Escape", "C-t", "C-a", "x", "-", "Z"):
            with self.subTest(token=token):
                self.assertIsNone(driver._csi_u_bytes(token))


class SendKeysTest(unittest.TestCase):
    def test_modified_special_keys_go_through_hex_mode(self) -> None:
        with patch.object(driver, "_tmux") as tmux:
            driver.send_keys(object(), ["C-Enter", "x"])
        self.assertEqual(
            [call.args[1:] for call in tmux.call_args_list],
            [
                ("send-keys", "-t", "magit", "-H",
                 "1b", "5b", "31", "33", "3b", "35", "75"),
                ("send-keys", "-t", "magit", "-l", "--", "x"),
            ],
        )

    def test_plain_named_and_alt_keys_are_unchanged(self) -> None:
        with patch.object(driver, "_tmux") as tmux:
            driver.send_keys(object(), ["BSpace", "M-3"])
        self.assertEqual(
            [call.args[1:] for call in tmux.call_args_list],
            [
                ("send-keys", "-t", "magit", "BSpace"),
                ("send-keys", "-t", "magit", "M-3"),
            ],
        )


class LauncherTest(unittest.TestCase):
    def test_environment_stays_out_of_the_tmux_command(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            env = {"PATH": "/usr/bin:/bin", "BIG": "x" * 10000}
            script = driver._launcher_script(run, env)
            text = script.read_text(encoding="utf-8")
            mode = script.stat().st_mode & 0o777
            command = driver._launcher_command(run)
        self.assertIn("env -i", text)
        self.assertIn("BIG=" + "x" * 10000, text)
        self.assertIn("--listen", text)
        self.assertIn(str(run.socket_path), text)
        self.assertIn("nvim.exit", text)
        self.assertIn("nvim.stderr", text)
        self.assertIn('exit "$status"', text)
        self.assertEqual(mode, 0o600)
        self.assertLess(len(command), 512)
        self.assertNotIn("BIG", command)
        self.assertNotIn(str(run.repo_path), command)


class StartupReportTest(unittest.TestCase):
    def test_report_combines_socket_exit_stderr_and_connect(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            Path(driver._launcher_path(run)).write_text(
                "#!/bin/sh\n", encoding="utf-8"
            )
            socket = startup.SocketState(
                path=str(run.socket_path),
                length=len(str(run.socket_path)),
                exists=True,
                is_socket=True,
                mode="0o600",
            )
            with (
                patch.object(driver.startup, "inspect_socket", return_value=socket),
                patch.object(driver, "_pane_dead", return_value=True),
                patch.object(driver.startup, "read_exit_status", return_value=1),
                patch.object(
                    driver.startup, "read_tail", return_value=["boom", "detail"]
                ),
                patch.object(
                    driver.startup,
                    "connect_probe",
                    return_value=startup.ConnectState(
                        True, False, "E247: no server"
                    ),
                ),
            ):
                report = driver.startup_report(run)
        self.assertIn("is a socket; mode 0o600", report)
        self.assertIn("nvim pane: exited", report)
        self.assertIn("nvim exit status: 1", report)
        self.assertIn("connect probe: failed: E247: no server", report)
        self.assertIn("  boom", report)
        self.assertIn("tmux launcher command:", report)

    def test_non_socket_skips_the_connect_probe(self) -> None:
        socket = startup.SocketState(
            path="/tmp/nvim.sock",
            length=14,
            exists=True,
            is_socket=False,
            mode="0o644",
        )
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            with (
                patch.object(driver.startup, "inspect_socket", return_value=socket),
                patch.object(driver, "_pane_dead", return_value=False),
                patch.object(driver.startup, "read_exit_status", return_value=None),
                patch.object(driver.startup, "read_tail", return_value=[]),
                patch.object(driver.startup, "connect_probe") as probe,
            ):
                report = driver.startup_report(run)
        probe.assert_not_called()
        self.assertIn("not attempted: no socket to contact", report)


class WaitSocketTest(unittest.TestCase):
    def test_pane_death_is_a_startup_error_with_the_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            with (
                patch.object(driver, "_pane_dead", return_value=True),
                patch.object(driver, "startup_report", return_value="REPORT"),
                patch.object(driver.time, "sleep"),
            ):
                with self.assertRaises(driver.StartupError) as caught:
                    driver._wait_for_socket(run)
        self.assertIn("RPC socket", str(caught.exception))
        self.assertIn("REPORT", str(caught.exception))

    def test_timeout_is_a_startup_error_with_the_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            with patch.object(driver, "startup_report", return_value="REPORT"):
                with self.assertRaises(driver.StartupError) as caught:
                    driver._wait_for_socket(run, timeout_ms=0)
        self.assertIn("within 0 ms", str(caught.exception))
        self.assertIn("REPORT", str(caught.exception))


class StartPathTest(unittest.TestCase):
    def test_overlong_socket_path_is_rejected_before_launch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory) / "run"
            state = startup.SocketState(
                path=str(run_dir / "nvim.sock"),
                length=startup.SOCKET_PATH_LIMIT + 1,
            )
            with (
                patch.object(driver, "check_tools"),
                patch.object(driver, "run_dir", return_value=run_dir),
                patch.object(driver.fixtures, "build"),
                patch.object(driver.startup, "inspect_socket", return_value=state),
                patch.object(driver, "_tmux") as tmux,
                patch.object(driver, "_cleanup"),
            ):
                with self.assertRaises(driver.StartupError) as caught:
                    driver.start("clean")
        self.assertIn("too long", str(caught.exception))
        tmux.assert_not_called()


if __name__ == "__main__":
    unittest.main()
