"""Unit tests for the ANSI decoder (§A.1, U0.4)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import decoder  # noqa: E402


def run(row=0, col=0, text="", fg=None, bg=None, bold=False, dim=False, reverse=False):
    return {
        "row": row,
        "col": col,
        "text": text,
        "fg": fg,
        "bg": bg,
        "bold": bold,
        "dim": dim,
        "reverse": reverse,
    }


class DecoderTest(unittest.TestCase):
    def test_plain_text(self) -> None:
        self.assertEqual(decoder.decode("hello"), [run(text="hello")])

    def test_newlines_start_new_rows(self) -> None:
        self.assertEqual(
            decoder.decode("ab\ncd"),
            [run(text="ab"), run(row=1, text="cd")],
        )

    def test_16_colour_foreground_and_background(self) -> None:
        self.assertEqual(
            decoder.decode("\x1b[31;42mred on green\x1b[0m plain"),
            [
                run(text="red on green", fg="red", bg="green"),
                run(col=12, text=" plain"),
            ],
        )

    def test_bright_16_colour(self) -> None:
        self.assertEqual(
            decoder.decode("\x1b[91;104mbright\x1b[39;49m normal"),
            [
                run(text="bright", fg="bright-red", bg="bright-blue"),
                run(col=6, text=" normal"),
            ],
        )

    def test_256_colour(self) -> None:
        self.assertEqual(
            decoder.decode("\x1b[38;5;196;48;5;21mx"),
            [run(text="x", fg="256:196", bg="256:21")],
        )

    def test_truecolor(self) -> None:
        self.assertEqual(
            decoder.decode("\x1b[38;2;10;20;30;48;2;1;2;3mx"),
            [run(text="x", fg="rgb:10,20,30", bg="rgb:1,2,3")],
        )

    def test_colon_sub_parameters(self) -> None:
        self.assertEqual(
            decoder.decode("\x1b[38:2::9:8:7mx"),
            [run(text="x", fg="rgb:9,8,7")],
        )

    def test_bold_and_dim_resets(self) -> None:
        self.assertEqual(
            decoder.decode(
                "\x1b[1mbold\x1b[22mplain\x1b[2mdim\x1b[23mnormal"
            ),
            [
                run(text="bold", bold=True),
                run(col=4, text="plain"),
                run(col=9, text="dim", dim=True),
                run(col=12, text="normal"),
            ],
        )

    def test_reverse(self) -> None:
        self.assertEqual(
            decoder.decode("\x1b[7mrev\x1b[27mplain"),
            [run(text="rev", reverse=True), run(col=3, text="plain")],
        )

    def test_full_reset(self) -> None:
        self.assertEqual(
            decoder.decode("\x1b[1;2;7;31mx\x1b[0my"),
            [
                run(text="x", fg="red", bold=True, dim=True, reverse=True),
                run(col=1, text="y"),
            ],
        )

    def test_other_csi_sequences_are_skipped(self) -> None:
        self.assertEqual(decoder.decode("\x1b[2Khello"), [run(text="hello")])
        self.assertEqual(decoder.decode("\x1b[?25lhi"), [run(text="hi")])

    def test_osc_title_is_skipped(self) -> None:
        self.assertEqual(decoder.decode("\x1b]0;title\x07hi"), [run(text="hi")])

    def test_runs_are_merged_by_style(self) -> None:
        self.assertEqual(
            decoder.decode("\x1b[32mab\x1b[32mcd"),
            [run(text="ab", fg="green"), run(col=2, text="cd", fg="green")],
        )
        # An escape breaks the run even when the style is unchanged.
        self.assertEqual(
            decoder.decode("ab\x1b[0mcd"),
            [run(text="ab"), run(col=2, text="cd")],
        )

    def test_runs_containing(self) -> None:
        runs = decoder.decode("\x1b[2m(--all)\x1b[0m Commit")
        matches = decoder.runs_containing(runs, "(--all)")
        self.assertEqual(len(matches), 1)
        self.assertTrue(matches[0]["dim"])
        self.assertIsNone(matches[0]["fg"])
        self.assertEqual(matches[0]["col"], 0)

    def test_carriage_return_resets_column(self) -> None:
        self.assertEqual(
            decoder.decode("abc\rX")[:2],
            [run(text="abc"), run(col=0, text="X")],
        )


if __name__ == "__main__":
    unittest.main()
