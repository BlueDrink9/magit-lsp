"""Decode `tmux capture-pane -p -e` output into styled runs.

A run is a horizontal stretch of text with one style:

    {"row": 0, "col": 0, "text": "hello",
     "fg": "red", "bg": None, "bold": False, "dim": False, "reverse": False}

`fg`/`bg` are `None` for the terminal default. Colour values are colour
names (`red`, `bright-red`), `256:<n>` for indexed colours, or
`rgb:<r>,<g>,<b>` for truecolor. Rows and columns are 0-based; `col` counts
characters, not bytes. Styles persist across newlines, as in a terminal.
"""

from __future__ import annotations

import re
from typing import Iterable

# SGR parameter lists may use `;` or the colon sub-parameter form. Other CSI
# sequences (erase-line, cursor movement, private modes) are skipped.
_CSI_RE = re.compile(r"\x1b\[([0-9;:?<=>]*)([@-~])")
_OSC_RE = re.compile(r"\x1b\](?:[^\x07\x1b]|\x1b(?!\\))*(?:\x07|\x1b\\)")

_BASE_COLOURS = (
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
)


def _style(fg, bg, bold, dim, reverse) -> tuple:
    return (fg, bg, bold, dim, reverse)


def _apply_sgr(
    params: str,
    fg: str | None,
    bg: str | None,
    bold: bool,
    dim: bool,
    reverse: bool,
) -> tuple:
    """Apply one SGR sequence and return the new style tuple."""
    values = [int(part) for part in re.split(r"[;:]", params) if part != ""]
    if not values:
        values = [0]
    index = 0
    while index < len(values):
        code = values[index]
        index += 1
        if code == 0:
            fg, bg, bold, dim, reverse = None, None, False, False, False
        elif code == 1:
            bold = True
        elif code == 2:
            dim = True
        elif code == 3 or code == 4 or code == 5 or code == 9:
            pass  # italic, underline, blink, strikethrough: not in the model
        elif code == 7:
            reverse = True
        elif code == 22:
            bold = False
            dim = False
        elif code == 23:
            dim = False
        elif code == 27:
            reverse = False
        elif code == 39:
            fg = None
        elif code == 49:
            bg = None
        elif 30 <= code <= 37:
            fg = _BASE_COLOURS[code - 30]
        elif 90 <= code <= 97:
            fg = "bright-" + _BASE_COLOURS[code - 90]
        elif 40 <= code <= 47:
            bg = _BASE_COLOURS[code - 40]
        elif 100 <= code <= 107:
            bg = "bright-" + _BASE_COLOURS[code - 100]
        elif code in (38, 48):
            value, consumed = _extended_colour(values, index)
            index += consumed
            if value is not None:
                if code == 38:
                    fg = value
                else:
                    bg = value
        # Unknown codes are ignored rather than aborting the run.
    return _style(fg, bg, bold, dim, reverse)


def _extended_colour(values: list[int], index: int) -> tuple[str | None, int]:
    """Read the value of `38`/`48`; return the colour and values consumed."""
    if index >= len(values):
        return None, 0
    mode = values[index]
    if mode == 5 and index + 1 < len(values):
        return f"256:{values[index + 1]}", 2
    if mode == 2 and index + 3 < len(values):
        red, green, blue = values[index + 1 : index + 4]
        return f"rgb:{red},{green},{blue}", 4
    # Colours with a colour-space identifier (`38:2::r:g:b`) lose the empty
    # field when parsing, so the values above are already the right ones.
    return None, 1


def decode(text: str) -> list[dict]:
    """Turn ANSI-coloured pane text into a list of styled runs."""
    runs: list[dict] = []
    current: dict | None = None
    row = 0
    col = 0
    fg = bg = None
    bold = dim = reverse = False

    def flush() -> None:
        nonlocal current
        if current is not None and current["text"]:
            runs.append(current)
        current = None

    def emit(chunk: str) -> None:
        nonlocal current, col
        if not chunk:
            return
        style = _style(fg, bg, bold, dim, reverse)
        if (
            current is not None
            and _style(
                current["fg"],
                current["bg"],
                current["bold"],
                current["dim"],
                current["reverse"],
            )
            == style
            and current["row"] == row
            and current["col"] + len(current["text"]) == col
        ):
            current["text"] += chunk
        else:
            flush()
            current = {
                "row": row,
                "col": col,
                "text": chunk,
                "fg": fg,
                "bg": bg,
                "bold": bold,
                "dim": dim,
                "reverse": reverse,
            }
        col += len(chunk)

    index = 0
    length = len(text)
    while index < length:
        char = text[index]
        if char == "\x1b":
            flush()
            if text.startswith("[", index + 1):
                match = _CSI_RE.match(text, index)
                if match:
                    if match.group(2) == "m":
                        fg, bg, bold, dim, reverse = _apply_sgr(
                            match.group(1), fg, bg, bold, dim, reverse
                        )
                    index = match.end()
                    continue
                index += 1
                continue
            if text.startswith("]", index + 1):
                match = _OSC_RE.match(text, index)
                if match:
                    index = match.end()
                    continue
                index += 1
                continue
            index += 1
            continue
        if char == "\n":
            flush()
            row += 1
            col = 0
            index += 1
            continue
        if char == "\r":
            flush()
            col = 0
            index += 1
            continue
        if char == "\t":
            emit(" " * (8 - col % 8))
            index += 1
            continue
        # Collect a whole run of plain text before checking styles again.
        end = index + 1
        while end < length and text[end] not in "\x1b\n\r\t":
            end += 1
        emit(text[index:end])
        index = end

    flush()
    return runs


def runs_containing(runs: Iterable[dict], needle: str) -> list[dict]:
    """Return the runs whose text contains `needle`."""
    return [run for run in runs if needle in run["text"]]
