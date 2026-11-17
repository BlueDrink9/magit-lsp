"""Unit tests for the tmux/Neovim driver helpers (spec §A.1)."""

from __future__ import annotations

import base64
import fcntl
import json
import os
import pty
import re
import select
import shutil
import subprocess
import sys
import tempfile
import termios
import time
import unittest
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import driver, startup  # noqa: E402
import test_plain_presets  # noqa: E402


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


class TmuxTimeoutTest(unittest.TestCase):
    def test_tmux_subprocess_has_finite_timeout(self) -> None:
        run = make_run("/tmp/test-run")
        result = subprocess.CompletedProcess([], 0, stdout="", stderr="")
        with patch.object(driver.subprocess, "run", return_value=result) as subprocess_run:
            driver._tmux(run, "list-sessions")
        self.assertEqual(subprocess_run.call_args.kwargs["timeout"], driver.TMUX_TIMEOUT)

    def test_cleanup_continues_after_tmux_timeout(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            with (
                patch.object(driver, "_tmux", side_effect=subprocess.TimeoutExpired("tmux", 5)),
                patch.object(driver, "_forget_last_run") as forget,
            ):
                driver._cleanup(run)
            self.assertFalse(run.dir.exists())
            forget.assert_called_once_with(run.id)


class StateJsonTest(unittest.TestCase):
    def test_pretty_by_default_and_compact_when_indent_is_none(self) -> None:
        with patch.object(driver, "snapshot", return_value={"a": 1, "b": [2]}):
            pretty = driver.state_json(object())
            compact = driver.state_json(object(), indent=None)
        self.assertIn("\n", pretty)
        self.assertEqual(compact, '{"a":1,"b":[2]}')


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


class WaitBarrierTest(unittest.TestCase):
    def test_polls_again_until_all_barriers_hold_in_one_iteration(self) -> None:
        ready = {"cursor": {"row": 1, "col": 0}, "view_stack": [{"id": "v1"}]}
        with (
            patch.object(driver, "capture", side_effect=["ready", "missing", "ready"]),
            patch.object(driver, "snapshot", side_effect=[{}, ready, ready]) as snapshot,
            patch.object(driver.time, "monotonic", return_value=0),
            patch.object(driver.time, "sleep") as sleep,
        ):
            self.assertEqual(driver.wait_barriers(
                object(), text="ready", cursor=(1, 0), view="v1",
            ), [])
        self.assertEqual(snapshot.call_count, 3)
        self.assertEqual(sleep.call_count, 2)

    def test_barriers_share_one_deadline(self) -> None:
        with (
            patch.object(driver, "capture", return_value="missing") as capture,
            patch.object(driver, "snapshot", return_value={}),
            patch.object(driver.time, "monotonic", side_effect=[0, 0.01, 0.02]),
            patch.object(driver.time, "sleep") as sleep,
        ):
            pending = driver.wait_barriers(
                object(), text="ready", cursor=(1, 0), view="v1", timeout_ms=20,
            )
        self.assertEqual(len(pending), 3)
        self.assertEqual(capture.call_count, 2)
        sleep.assert_called_once()

    def test_text_barrier_returns_empty_when_visible(self) -> None:
        with patch.object(driver, "capture", return_value="a prompt here"):
            self.assertEqual(
                driver.wait_barriers(object(), text="prompt", timeout_ms=0), []
            )

    def test_cursor_barrier_matches_row_and_optional_column(self) -> None:
        state = {"cursor": {"row": 4, "col": 7}}
        with patch.object(driver, "snapshot", return_value=state):
            self.assertEqual(
                driver.wait_barriers(object(), cursor=(4, None), timeout_ms=0), []
            )
            self.assertEqual(
                driver.wait_barriers(object(), cursor=(4, 7), timeout_ms=0), []
            )
            self.assertTrue(
                driver.wait_barriers(object(), cursor=(4, 8), timeout_ms=0)
            )

    def test_view_barrier_matches_id_suffix_title_and_kind(self) -> None:
        state = {
            "view_stack": [
                {"id": "7\0v1", "kind": "menu.commit", "title": "Commit"},
            ]
        }
        with patch.object(driver, "snapshot", return_value=state):
            for wanted in ("7\0v1", "v1", "Commit", "menu.commit"):
                with self.subTest(wanted=wanted):
                    self.assertEqual(
                        driver.wait_barriers(
                            object(), view=wanted, timeout_ms=0
                        ),
                        [],
                    )
            self.assertTrue(
                driver.wait_barriers(object(), view="status", timeout_ms=0)
            )

    def test_all_barriers_must_hold_together(self) -> None:
        state = {
            "cursor": {"row": 1, "col": 0},
            "view_stack": [{"id": "v1", "kind": "status", "title": "Status"}],
        }
        with (
            patch.object(driver, "capture", return_value="main text"),
            patch.object(driver, "snapshot", return_value=state),
        ):
            self.assertEqual(
                driver.wait_barriers(
                    object(),
                    text="main",
                    cursor=(1, 0),
                    view="Status",
                    timeout_ms=0,
                ),
                [],
            )
            self.assertTrue(
                driver.wait_barriers(
                    object(),
                    text="missing",
                    cursor=(1, 0),
                    view="Status",
                    timeout_ms=0,
                )
            )

    def test_timeout_reports_every_unmet_barrier(self) -> None:
        state = {"cursor": {"row": 0, "col": 0}, "view_stack": []}
        with (
            patch.object(driver, "capture", return_value="nothing"),
            patch.object(driver, "snapshot", return_value=state),
        ):
            pending = driver.wait_barriers(
                object(),
                text="prompt",
                cursor=(3, None),
                view="status",
                timeout_ms=0,
            )
        self.assertEqual(len(pending), 3)
        self.assertIn("screen text 'prompt'", pending[0])
        self.assertIn("cursor (0, 0)", pending[1])
        self.assertIn("view 'status'", pending[2])

    def test_missing_view_stack_is_reported(self) -> None:
        with patch.object(driver, "snapshot", return_value={"cursor": {}}):
            pending = driver.wait_barriers(object(), view="v1", timeout_ms=0)
        self.assertEqual(pending, ["view 'v1' (snapshot has no view_stack)"])

    def test_parse_cursor(self) -> None:
        self.assertEqual(driver.parse_cursor("3"), (3, None))
        self.assertEqual(driver.parse_cursor("3:7"), (3, 7))
        for value in ("", "3:", "x", "-1", "1:2:3"):
            with self.subTest(value=value):
                with self.assertRaises(driver.DriverError):
                    driver.parse_cursor(value)

    def test_screen_excerpt_keeps_head_and_tail(self) -> None:
        screen = "\n".join(f"line {number}" for number in range(20)) + "\n\n"
        excerpt = driver.screen_excerpt(screen, limit=6)
        self.assertIn("line 0", excerpt)
        self.assertIn("line 19", excerpt)
        self.assertIn("…", excerpt)
        self.assertNotIn("line 9", excerpt)


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
    def test_ctrl_shift_character_becomes_csi_u_bytes(self) -> None:
        self.assertEqual(
            driver.keys.csi_u_bytes("C-S-x"),
            ["1b", "5b", "31", "32", "30", "3b", "36", "75"],
        )

    def test_ctrl_enter_becomes_csi_u_bytes(self) -> None:
        self.assertEqual(
            driver.keys.csi_u_bytes("C-Enter"),
            ["1b", "5b", "31", "33", "3b", "35", "75"],
        )

    def test_profile_spellings_normalize(self) -> None:
        expected = ["1b", "5b", "31", "33", "3b", "35", "75"]
        for token in ("C-Enter", "ctrl+Enter", "Ctrl+Enter", "control-Enter"):
            with self.subTest(token=token):
                self.assertEqual(driver.keys.csi_u_bytes(token), expected)

    def test_shifted_tab_stays_on_the_terminal_backtab(self) -> None:
        for token in ("shift+Tab", "S-Tab", "Shift+Tab", "shift+tab"):
            with self.subTest(token=token):
                self.assertIsNone(driver.keys.csi_u_bytes(token))
                self.assertFalse(driver.is_key_name(token))

    def test_ctrl_shift_tab_keeps_csi_u_bytes(self) -> None:
        self.assertEqual(
            driver.keys.csi_u_bytes("C-S-Tab"),
            ["1b", "5b", "39", "3b", "36", "75"],
        )

    def test_ctrl_arrow_uses_the_legacy_form(self) -> None:
        self.assertEqual(
            driver.keys.csi_u_bytes("C-Left"), ["1b", "5b", "31", "3b", "35", "44"]
        )

    def test_arrow_aliases(self) -> None:
        self.assertEqual(
            driver.keys.csi_u_bytes("ctrl+ArrowDown"),
            ["1b", "5b", "31", "3b", "35", "42"],
        )

    def test_modified_page_keys_use_tilde_form(self) -> None:
        self.assertEqual(
            driver.keys.csi_u_bytes("C-PPage"), ["1b", "5b", "35", "3b", "35", "7e"]
        )

    def test_alt_stays_on_tmux_named_keys(self) -> None:
        for token in ("M-3", "M-4", "alt+ArrowDown", "alt+x"):
            with self.subTest(token=token):
                self.assertIsNone(driver.keys.csi_u_bytes(token))

    def test_untranslated_tokens_keep_their_existing_path(self) -> None:
        for token in ("Enter", "Tab", "BTab", "Escape", "C-t", "C-a", "x", "-", "Z"):
            with self.subTest(token=token):
                self.assertIsNone(driver.keys.csi_u_bytes(token))


class SendKeysTest(unittest.TestCase):
    def test_opt_in_probe_records_each_token_and_exact_tmux_arguments(self) -> None:
        import tempfile
        from types import SimpleNamespace

        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {"MAGIT_LSP_UI_PROBE": "1"}):
            run = SimpleNamespace(dir=Path(directory))
            with patch.object(driver, "_tmux") as tmux:
                driver.send_keys(run, ["Enter", "Enter"])
            records = [json.loads(line) for line in (Path(directory) / "input-probe.jsonl").read_text().splitlines()]
        self.assertEqual([record["token"] for record in records], ["Enter", "Enter"])
        self.assertEqual([record["tmux_args"] for record in records], [
            ["send-keys", "-t", "magit", "Enter"], ["send-keys", "-t", "magit", "Enter"]])
        self.assertEqual([call.args[1:] for call in tmux.call_args_list], [tuple(r["tmux_args"]) for r in records])

    def test_ctrl_shift_character_goes_through_hex_mode(self) -> None:
        with patch.object(driver, "_tmux") as tmux:
            driver.send_keys(object(), ["C-S-x"])
        self.assertEqual(
            [call.args[1:] for call in tmux.call_args_list],
            [("send-keys", "-t", "magit", "-H", "1b", "5b", "31", "32", "30", "3b", "36", "75")],
        )

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

    def test_shifted_tab_goes_through_the_backtab_name(self) -> None:
        with patch.object(driver, "_tmux") as tmux:
            driver.send_keys(object(), ["shift+Tab", "S-Tab"])
        self.assertEqual(
            [call.args[1:] for call in tmux.call_args_list],
            [
                ("send-keys", "-t", "magit", "BTab"),
                ("send-keys", "-t", "magit", "BTab"),
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
    def test_idle_probe_preserves_false_and_unexpected_results(self) -> None:
        for stdout, outcome in (("false\n", "false"), ("wat\n", "unexpected")):
            with self.subTest(stdout=stdout):
                result = subprocess.CompletedProcess(
                    [], 0, stdout=stdout, stderr=""
                )
                with patch.object(driver, "_nvim", return_value=result):
                    probe = driver._idle_probe(make_run("/tmp/test-run"))
                self.assertEqual(probe.outcome, outcome)
                self.assertEqual(probe.stdout, stdout.strip())

    def test_idle_probe_preserves_remote_expression_error(self) -> None:
        result = subprocess.CompletedProcess(
            [], 2, stdout="", stderr="Lua: E5108: debug unavailable\n"
        )
        with patch.object(driver, "_nvim", return_value=result):
            probe = driver._idle_probe(make_run("/tmp/test-run"))
        self.assertEqual(probe.outcome, "error")
        self.assertEqual(probe.returncode, 2)
        self.assertIn("debug unavailable", probe.stderr)

    def test_idle_probe_preserves_rpc_timeout(self) -> None:
        timeout = subprocess.TimeoutExpired([], 2.0, output=b"", stderr=b"late")
        with patch.object(driver, "_nvim", side_effect=timeout):
            probe = driver._idle_probe(make_run("/tmp/test-run"))
        self.assertEqual(probe.outcome, "timeout")
        self.assertEqual(probe.stderr, "late")

    def test_startup_state_is_formatted_as_readable_bounded_fields(self) -> None:
        payload = {
            "state": {
                "requests": 2,
                "queued": 1,
                "prompts": 0,
                "scheduled": 3,
                "progress": 1,
            },
            "views": [{"id": "v1", "kind": "status", "version": 4, "acked": 3}],
            "messages": "recent warning\n",
        }
        result = subprocess.CompletedProcess(
            [], 0, stdout=json.dumps(payload), stderr=""
        )
        with patch.object(driver, "_nvim", return_value=result):
            lines = driver._client_startup_state(make_run("/tmp/test-run"))
        report = "\n".join(lines)
        self.assertIn("requests=2, queued=1, prompts=0, scheduled=3, progress=1", report)
        self.assertIn("version=4, acked=3", report)
        self.assertIn("recent warning", report)
        self.assertNotIn("{", report)

    def test_timeout_report_keeps_last_false_idle_result(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            run.socket_path.touch()
            probe = startup.IdleProbe("false", 0, "false")
            socket = startup.SocketState(
                str(run.socket_path), len(str(run.socket_path)), True, True
            )
            with (
                patch.object(driver, "_idle_probe", return_value=probe),
                patch.object(driver, "_pane_dead", return_value=False),
                patch.object(driver.startup, "inspect_socket", return_value=socket),
                patch.object(
                    driver.time, "monotonic", side_effect=[0.0, 0.0, 2.0]
                ),
                patch.object(driver.time, "sleep"),
                patch.object(
                    driver, "startup_report", side_effect=lambda run, **kw: kw["idle_probe"].summary()
                ) as report,
            ):
                with self.assertRaises(driver.StartupError) as caught:
                    driver._wait_for_socket(run, timeout_ms=1000)
        self.assertIn("socket answered, but idle readiness", str(caught.exception))
        self.assertIn("returned false", str(caught.exception))
        self.assertIs(report.call_args.kwargs["idle_probe"], probe)

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
        self.assertIn("startup readiness", str(caught.exception))
        self.assertIn("REPORT", str(caught.exception))

    def test_timeout_is_a_startup_error_with_the_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            with patch.object(driver, "startup_report", return_value="REPORT"):
                with self.assertRaises(driver.StartupError) as caught:
                    driver._wait_for_socket(run, timeout_ms=0)
        self.assertIn("within 0 ms", str(caught.exception))
        self.assertIn("REPORT", str(caught.exception))


class NvimHelperCommandTest(unittest.TestCase):
    def test_plain_preset_cleanup_failure_preserves_primary_exception(self) -> None:
        primary = AssertionError("primary assertion failed")
        with patch.object(
            test_plain_presets, "teardown", side_effect=RuntimeError("cleanup failed")
        ):
            with self.assertRaises(AssertionError) as caught:
                test_plain_presets.teardown_preserving_primary(object(), primary)
        self.assertIs(caught.exception, primary)
        self.assertEqual(str(caught.exception), "primary assertion failed")
        self.assertIn("process cleanup also failed: cleanup failed", caught.exception.__notes__)

    def test_plain_preset_teardown_reaps_owned_descendant(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "ready"
            child = subprocess.Popen(
                [sys.executable, "-c", "import subprocess,sys,time,pathlib; p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); pathlib.Path(sys.argv[1]).write_text(str(p.pid)); time.sleep(60)", str(marker)],
                start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            deadline = time.monotonic() + 5
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(.01)
            self.assertTrue(marker.exists(), "descendant readiness deadline expired")
            descendant = int(marker.read_text())
            test_plain_presets.teardown(child)
            with self.assertRaises(ProcessLookupError):
                os.kill(descendant, 0)

    def test_rpc_helper_is_headless_and_has_no_terminal_input(self) -> None:
        result = subprocess.CompletedProcess([], 0, stdout="1", stderr="")
        with patch.object(driver.subprocess, "run", return_value=result) as run:
            driver._nvim(make_run("/tmp"), "--remote-expr", "1")
        self.assertIn("--headless", run.call_args.args[0])
        self.assertIs(run.call_args.kwargs["stdin"], subprocess.DEVNULL)


@unittest.skipUnless(
    shutil.which("nvim") and hasattr(termios, "TIOCSCTTY"),
    "requires Neovim and PTYs",
)
class RpcHelperTtyTest(unittest.TestCase):
    def test_helpers_do_not_enter_tui_or_leak_terminal_queries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            socket = base / "nvim.sock"
            trace = base / "trace.jsonl"
            env = os.environ.copy()
            env.update({"MAGIT_LSP_TRACE": str(trace), "TERM": "xterm-256color"})
            server = subprocess.Popen(
                ["nvim", "--headless", "--listen", str(socket), "-u", str(driver.MINIMAL_INIT)],
                cwd=ROOT,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
            )
            master = slave = None
            try:
                deadline = time.monotonic() + 5
                while not socket.exists() and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue(socket.exists(), "Neovim test RPC server did not start")

                # A real terminal on stdin/stdout reproduces the former helper's TUI path.
                master, slave = pty.openpty()

                def attach_controlling_terminal() -> None:
                    os.setsid()
                    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

                old = subprocess.Popen(
                    [
                        "nvim",
                        "--server",
                        str(socket),
                        "--remote-expr",
                        "luaeval('true')",
                    ],
                    stdin=slave,
                    stdout=slave,
                    stderr=subprocess.PIPE,
                    env=env,
                    preexec_fn=attach_controlling_terminal,
                    close_fds=True,
                )
                try:
                    old_stderr = old.communicate(timeout=5)[1]
                except subprocess.TimeoutExpired:
                    old.kill()
                    old.communicate(timeout=2)
                    raise
                self.assertEqual(old.returncode, 0)
                self.assertEqual(old_stderr.strip(), b"true")
                terminal_chunks = []
                while select.select([master], [], [], 0.05)[0]:
                    terminal_chunks.append(os.read(master, 8192))
                terminal_traffic = b"".join(terminal_chunks)
                self.assertIn(b"\x1b[", terminal_traffic)

                run = make_run(directory)
                result = driver._nvim(
                    run, "--remote-expr", "luaeval('true')"
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), "true")
                self.assertEqual(result.stderr, "")
                self.assertEqual(driver._idle_probe(run).outcome, "true")
                self.assertTrue(
                    any(
                        "client work:" in line
                        for line in driver._client_startup_state(run)
                    )
                )
                self.assertTrue(startup.connect_probe(socket).ok)
                failure = driver._nvim(
                    run,
                    "--remote-expr",
                    "luaeval('error(\"injected RPC error\")')",
                )
                self.assertNotEqual(failure.returncode, 0)
                self.assertIn("injected RPC error", failure.stderr)
                self.assertFalse(select.select([master], [], [], 0.1)[0])
            finally:
                if master is not None:
                    os.close(master)
                if slave is not None:
                    os.close(slave)
                server.terminate()
                try:
                    server.communicate(timeout=3)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.communicate(timeout=2)


class StartPathTest(unittest.TestCase):
    def test_plain_launch_uses_plugin_free_init_file_and_role_wrappers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory) / "run"

            def build(_fixture, repo, home):
                repo.mkdir()
                (repo / ".git/info").mkdir(parents=True)
                home.mkdir()

            with (
                patch.object(driver, "check_tools"),
                patch.object(driver, "run_dir", return_value=run_dir),
                patch.object(driver.fixtures, "build", side_effect=build),
                patch.object(driver, "_tmux"),
                patch.object(driver, "_wait_for_socket"),
                patch.object(driver, "_wait_for_view"),
                patch.object(driver, "_update_last_run"),
            ):
                run = driver.start("clean", client="plain", size="80x24")
            self.assertEqual(run.client, "plain")
            self.assertEqual(json.loads((run.dir / "state.json").read_text())["client"], "plain")
            launcher = (run.dir / "launch-nvim.sh").read_text()
            editor = (run.dir / "editor-exec.sh").read_text()
            self.assertIn("MAGIT_LSP_UI_CLIENT=plain", launcher)
            self.assertIn(f"PLAIN_REPO={run.repo}", launcher)
            self.assertIn(str(driver.PLAIN_INIT), editor)
            self.assertIn(str(run.repo_path / ".magit"), editor)
            self.assertNotIn(str(driver.MINIMAL_INIT), editor)
            self.assertIn("server-exec.sh", launcher)
            self.assertIn("target/debug/magit-lsp", (run.dir / "server-exec.sh").read_text())

    def test_plain_probes_do_not_load_the_rich_adapter(self) -> None:
        run = make_run("/tmp/test-run")
        run.client = "plain"
        with patch.object(driver, "_expr", return_value='{"ready":true}') as expr:
            self.assertTrue(driver.snapshot(run)["ready"])
        self.assertIn("PlainUI.snapshot()", expr.call_args.args[1])
        with patch.object(driver, "_nvim", return_value=subprocess.CompletedProcess([], 0, "true", "")) as nvim:
            self.assertTrue(driver.idle(run))
        self.assertIn("PlainUI.idle()", nvim.call_args.args[2])
        payload = json.dumps({"ready": False, "idle": True, "applied_edits": 0,
                              "uri": "file:///repo/.magit", "messages": ["startup warning"]})
        with patch.object(driver, "_nvim", return_value=subprocess.CompletedProcess([], 0, payload, "")):
            report = "\n".join(driver._client_startup_state(run))
        self.assertIn("applied edits=0", report)
        self.assertIn("startup warning", report)
        self.assertNotIn("acked", report)

    def test_server_binary_defaults_to_checkout_debug_build(self) -> None:
        self._assert_server_binary(None, Path(driver.__file__).resolve().parents[2] / "target/debug/magit-lsp")

    def test_explicit_server_binary_override_survives_environment_isolation(self) -> None:
        self._assert_server_binary({"MAGIT_LSP_BIN": "/tmp/test-magit-lsp"}, Path("/tmp/test-magit-lsp"))

    def _assert_server_binary(self, extra_env: dict[str, str] | None, expected: Path) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory) / "run"
            state = startup.SocketState(path=str(run_dir / "nvim.sock"), length=1)
            captured: dict[str, str] = {}

            def capture(_run: object, env: dict[str, str]) -> None:
                captured.update(env)
                raise RuntimeError("stop after environment capture")

            with (
                patch.object(driver, "check_tools"),
                patch.object(driver, "run_dir", return_value=run_dir),
                patch.object(driver.fixtures, "build"),
                patch.object(driver.startup, "inspect_socket", return_value=state),
                patch.object(driver, "_launcher_script", side_effect=capture),
                patch.object(driver, "_cleanup"),
            ):
                with self.assertRaises(RuntimeError):
                    driver.start("clean", extra_env=extra_env)
        self.assertEqual(Path(captured["MAGIT_LSP_BIN"]), expected)

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


class NativeSessionTest(unittest.TestCase):
    def test_native_session_preserves_records_when_owned_process_survives(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, home, init, initial = (base / name for name in ("repo", "home", "init.lua", "start.txt"))
            repo.mkdir()
            home.mkdir()
            init.touch()
            initial.touch()
            observed = {}

            def launcher(run, _env, _config, _file, roles=None):
                observed["run"] = run
                for role, pid in (("editor", 456), ("server", 123)):
                    roles[role].write_text(f"{pid}\n", encoding="utf-8")
                    roles[role].with_suffix(".identity").write_text(
                        json.dumps({"pid": pid, "identity": f"birth-{pid}"}), encoding="utf-8")

            report = {"ownership": {"editor": 456, "server": 123}, "remaining_owned_pids": [123]}
            with (
                patch.object(driver, "check_tools"),
                patch.object(driver, "_launcher_script", side_effect=launcher),
                patch.object(driver, "_tmux"),
                patch.object(driver, "_wait_for_socket"),
                patch.object(driver, "stop", return_value=report),
            ):
                with driver.native_session(repo, home, init, initial, {}):
                    pass
            run = observed["run"]
            self.assertTrue(run.dir.is_dir())
            self.assertEqual((run.dir / "server.pid").read_text(encoding="utf-8"), "123\n")
            self.assertEqual(json.loads((run.dir / "server.identity").read_text(encoding="utf-8")),
                             {"pid": 123, "identity": "birth-123"})
            driver.shutil.rmtree(run.dir)

    def test_native_session_defers_directory_deletion_to_successful_stop(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, home, init, initial = (base / name for name in ("repo", "home", "init.lua", "start.txt"))
            repo.mkdir()
            home.mkdir()
            init.touch()
            initial.touch()
            observed = {}

            def launcher(run, *_args, **_kwargs):
                observed["run"] = run

            with (
                patch.object(driver, "check_tools"),
                patch.object(driver, "_launcher_script", side_effect=launcher),
                patch.object(driver, "_tmux"),
                patch.object(driver, "_wait_for_socket"),
                patch.object(driver, "_stop_roles", return_value={
                    "ownership": {"editor": 456, "server": 123}, "remaining_owned_pids": []}),
                patch.object(driver, "_forget_last_run"),
            ):
                with driver.native_session(repo, home, init, initial, {}):
                    pass
            self.assertFalse(observed["run"].dir.exists())

    def test_native_session_teardown_error_preserves_directory_and_body_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, home, init, initial = (base / name for name in ("repo", "home", "init.lua", "start.txt"))
            repo.mkdir()
            home.mkdir()
            init.touch()
            initial.touch()
            observed = {}

            def launcher(run, *_args, **_kwargs):
                observed["run"] = run
                (run.dir / "editor.pid").write_text("456\n", encoding="utf-8")
                (run.dir / "editor.identity").write_text('{"pid":456,"identity":"birth-456"}', encoding="utf-8")

            with (
                patch.object(driver, "check_tools"),
                patch.object(driver, "_launcher_script", side_effect=launcher),
                patch.object(driver, "_tmux"),
                patch.object(driver, "_wait_for_socket"),
                patch.object(driver, "stop", side_effect=RuntimeError("teardown failed")),
            ):
                with self.assertRaisesRegex(RuntimeError, "teardown failed"):
                    with driver.native_session(repo, home, init, initial, {}):
                        pass
            run = observed["run"]
            self.assertTrue(run.dir.is_dir())
            self.assertTrue((run.dir / "editor.pid").is_file())
            self.assertTrue((run.dir / "editor.identity").is_file())
            driver.shutil.rmtree(run.dir)

            with (
                patch.object(driver, "check_tools"),
                patch.object(driver, "_launcher_script", side_effect=launcher),
                patch.object(driver, "_tmux"),
                patch.object(driver, "_wait_for_socket"),
                patch.object(driver, "stop", side_effect=RuntimeError("teardown failed")),
            ):
                with self.assertRaisesRegex(ValueError, "body failed"):
                    with driver.native_session(repo, home, init, initial, {}):
                        raise ValueError("body failed")
            driver.shutil.rmtree(observed["run"].dir)

    def test_recorded_native_roles_use_shared_shutdown_before_editor_exit(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            run.socket_path.touch()
            (run.dir / "server.pid").write_text("123\n")

            def send(*_args, **_kwargs):
                run.socket_path.unlink()

            with (
                patch.object(driver, "_nvim", side_effect=send) as nvim,
                patch.object(driver, "_stop_roles", return_value={
                    "ownership": {"editor": 456, "server": 123}, "remaining_owned_pids": []}),
                patch.object(driver, "_tmux", return_value=subprocess.CompletedProcess([], 1)),
                patch.object(driver, "_forget_last_run"),
            ):
                report = driver.stop(run)
        self.assertIn(str(driver.NATIVE_SHUTDOWN), nvim.call_args.args[2])
        self.assertIn("(123)", nvim.call_args.args[2])
        self.assertTrue(report["cleanup_verified"])

    def test_recorded_child_is_stopped_before_editor_without_process_name_scans(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            (run.dir / "server.pid").write_text("123\n")
            (run.dir / "editor.pid").write_text("456\n")
            live = {123, 456}
            terminated = []

            def kill(pid, sig):
                if pid not in live:
                    raise ProcessLookupError()
                if sig:
                    terminated.append(pid)
                    live.remove(pid)

            def signal_owned(pid, record, sig):
                kill(pid, sig)
                return True

            with patch.object(driver.os, "kill", side_effect=kill), patch("ui_lib.process_identity.signal_owned", side_effect=signal_owned):
                report = driver._stop_roles(run)
        self.assertEqual(terminated, [123, 456])
        self.assertEqual(report, {"ownership": {"editor": 456, "server": 123}, "remaining_owned_pids": []})

    def test_recycled_role_pid_is_never_signaled(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            (run.dir / "server.pid").write_text("123\n")
            with patch.object(driver.os, "kill") as kill, patch("ui_lib.process_identity.signal_owned", return_value=False):
                report = driver._stop_roles(run)
            self.assertTrue(all(call.args[1] == 0 for call in kill.call_args_list))
            self.assertEqual(report["remaining_owned_pids"], [123])

    def test_process_birth_record_is_required_for_real_signal_probe(self) -> None:
        from ui_lib import process_identity
        with tempfile.TemporaryDirectory() as directory:
            record = Path(directory) / "server.pid"
            pid = os.getpid()
            birth = process_identity.identity(pid)
            self.assertIsNotNone(birth)
            self.assertFalse(process_identity.signal_owned(pid, record, 0))
            record.with_suffix(".identity").write_text(json.dumps({"pid": pid, "identity": "different-birth"}))
            self.assertFalse(process_identity.signal_owned(pid, record, 0))
            record.with_suffix(".identity").write_text(json.dumps({"pid": pid, "identity": birth}))
            self.assertTrue(process_identity.signal_owned(pid, record, 0))

    def test_missing_roles_cannot_claim_verified_cleanup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            with (
                patch.object(driver, "_tmux", return_value=subprocess.CompletedProcess([], 1)),
                patch.object(driver, "_forget_last_run"),
            ):
                report = driver.stop(run)
        self.assertFalse(report["cleanup_verified"])
        self.assertEqual(report["ownership"], {})

    def test_exec_wrappers_record_roles_and_missing_server_stays_unproven(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = make_run(directory)
            run.dir.mkdir(exist_ok=True)
            roles = {name: run.dir / f"{name}.pid" for name in ("editor", "server")}
            driver._launcher_script(run, {}, roles=roles)
            wrapper = run.dir / "editor-exec.sh"
            text = wrapper.read_text(encoding="utf-8")
            self.assertIn('"$$"', text)
            self.assertIn("exec ", text)
            self.assertNotIn("&", text)
            launcher = driver._launcher_script(run, {}, roles=roles).read_text(encoding="utf-8")
            self.assertIn(str(wrapper), launcher)
            self.assertNotIn("nvim --clean", launcher)
            server_wrapper = driver._role_wrapper(run, "server", roles["server"], ["server-bin", "--foreground"])
            server_text = server_wrapper.read_text(encoding="utf-8")
            self.assertIn('"$$"', server_text)
            self.assertIn("exec server-bin --foreground", server_text)
            self.assertFalse(roles["server"].exists())

    def test_rpc_readiness_uses_only_standard_expression(self) -> None:
        result = subprocess.CompletedProcess([], 0, stdout="1\n", stderr="")
        with patch.object(driver, "_nvim", return_value=result) as nvim:
            probe = driver._rpc_probe(make_run("/tmp"))
        self.assertEqual(probe.outcome, "true")
        self.assertEqual(nvim.call_args.args[1:], ("--remote-expr", "1"))

    def test_configurable_launch_and_readiness_failure_cleanup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, home, init, initial = (base / name for name in ("repo", "home", "init.lua", "start.txt"))
            repo.mkdir()
            home.mkdir()
            init.touch()
            initial.touch()
            launched: dict[str, object] = {}
            ownership: dict[str, int | None] = {}

            def launcher(run, env, config, file, roles=None):
                launched.update(run=run, env=env, config=config, file=file, roles=roles)
                roles["editor"].write_text("123\n", encoding="utf-8")

            with (
                patch.object(driver, "check_tools"),
                patch.object(driver, "_launcher_script", side_effect=launcher),
                patch.object(driver, "_tmux") as tmux,
                patch.object(driver, "_wait_for_socket", side_effect=driver.StartupError("not ready")),
                patch.object(driver, "stop") as stop,
            ):
                with self.assertRaisesRegex(driver.StartupError, "not ready"):
                    with driver.native_session(repo, home, init, initial, {"CUSTOM": "yes"}, size="120x100", ownership=ownership):
                        self.fail("readiness failure must not enter session")
            run = launched["run"]
            self.assertEqual(launched["env"], {"CUSTOM": "yes"})
            self.assertEqual(launched["config"], init)
            self.assertEqual(launched["file"], initial)
            self.assertEqual(run.repo_path, repo)
            self.assertEqual(run.home_path, home)
            self.assertEqual((run.cols, run.rows), (120, 100))
            self.assertTrue(any(call_args.args[1:3] == ("new-session", "-d") and call_args.args[5:9] == ("-x", "120", "-y", "100") for call_args in tmux.call_args_list))
            self.assertLessEqual(len(str(run.socket_path).encode()), startup.SOCKET_PATH_LIMIT)
            stop.assert_called_once_with(run)
            self.assertEqual(ownership, {"editor": 123, "server": None})

    def test_invalid_size_is_rejected_before_launch(self) -> None:
        with (
            patch.object(driver, "check_tools"),
            patch.object(driver, "_launcher_script") as launcher,
            patch.object(driver, "_tmux") as tmux,
        ):
            with self.assertRaises(driver.DriverError):
                with driver.native_session(Path("repo"), Path("home"), Path("init"), Path("start"), {}, size="bad"):
                    self.fail("invalid size must not enter session")
        launcher.assert_not_called()
        tmux.assert_not_called()


if __name__ == "__main__":
    unittest.main()
