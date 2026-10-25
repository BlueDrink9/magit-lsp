"""Unit tests for the tmux/Neovim driver helpers (spec §A.1)."""

from __future__ import annotations

import base64
import json
import re
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import driver  # noqa: E402


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


if __name__ == "__main__":
    unittest.main()
