"""Key tokens the UI harness sends, and their per-client spellings.

This module is the single source of truth for `scripts/ui keys --list`,
`scripts/ui keys --check` and the tmux/CSI-u bytes `driver.send_keys` sends.
Adding a named key here makes it listable, checkable and sendable at once; do
not keep a second copy of the key table in documentation or tests.

The harness accepts a small tmux-oriented vocabulary (for example `Escape`,
`BSpace`, `C-Enter`) while the repository's canonical §3.8 notation uses W3C
names and VSCode-style modifiers (`Escape`, `Backspace`, `ctrl+Enter`). The
table records both spellings so the command can show which one applies to
which client.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from difflib import get_close_matches


@dataclass(frozen=True)
class NamedKey:
    """One key with its harness spellings and per-client renderings.

    `name` is the canonical spelling shown first by `keys --list`; `spellings`
    are extra harness tokens that resolve to the same key. `tmux` is the
    `send-keys` name used when the key is unmodified or has no CSI-u encoding.
    `vscode` is the canonical §3.8 `keybindings.json` spelling.
    """

    name: str
    tmux: str
    vscode: str
    spellings: tuple[str, ...] = ()
    codepoint: str | None = None
    final: str | None = None
    tilde: str | None = None
    backtab: bool = False

    @property
    def tokens(self) -> tuple[str, ...]:
        """Every harness token that resolves to this key."""
        return (self.name, *self.spellings)


@dataclass(frozen=True)
class Modifier:
    """A modifier's aliases, CSI-u value and short tmux spelling, if supported."""

    name: str
    spellings: tuple[str, ...]
    csi_u_value: int | None
    tmux: str | None = None


@dataclass(frozen=True)
class Encoding:
    """How one token is sent: a tmux name, raw CSI-u bytes or literal text."""

    token: str
    kind: str
    value: str | tuple[str, ...]


# Order is the display order of `keys --list`. The CSI-u fields mirror the
# forms Neovim decodes: `ESC [ cp ; mod u`, `ESC [ 1 ; mod final` and
# `ESC [ n ; mod ~`. Delete deliberately has no CSI-u form: no terminal
# encoding was verified for it, so it stays on the tmux path.
NAMED_KEYS: tuple[NamedKey, ...] = (
    NamedKey("Enter", "Enter", "enter", ("CR",), codepoint="13"),
    NamedKey("Escape", "Escape", "escape", ("Esc",), codepoint="27"),
    NamedKey("Tab", "Tab", "tab", codepoint="9"),
    NamedKey(
        "shift+Tab", "BTab", "shift+tab", ("S-Tab", "BTab"), backtab=True
    ),
    NamedKey("Space", "Space", "space", codepoint="32"),
    NamedKey("Backspace", "BSpace", "backspace", ("BSpace",), codepoint="127"),
    NamedKey("Delete", "Delete", "delete", ("DC",)),
    NamedKey("Insert", "Insert", "insert", ("IC",), tilde="2"),
    NamedKey("Home", "Home", "home", final="H"),
    NamedKey("End", "End", "end", final="F"),
    NamedKey("PageUp", "PPage", "pageup", ("PPage",), tilde="5"),
    NamedKey("PageDown", "NPage", "pagedown", ("NPage",), tilde="6"),
    NamedKey("ArrowUp", "Up", "up", ("Up",), final="A"),
    NamedKey("ArrowDown", "Down", "down", ("Down",), final="B"),
    NamedKey("ArrowLeft", "Left", "left", ("Left",), final="D"),
    NamedKey("ArrowRight", "Right", "right", ("Right",), final="C"),
    *(
        NamedKey(f"F{number}", f"F{number}", f"f{number}")
        for number in range(1, 13)
    ),
)

MODIFIERS: tuple[Modifier, ...] = (
    Modifier("ctrl", ("C", "ctrl", "control"), 4, "C"),
    Modifier("shift", ("S", "shift"), 1, "S"),
    Modifier("alt", ("M", "alt"), None, "M"),
    Modifier("meta", ("meta",), None),
)

_BY_TOKEN: dict[str, NamedKey] = {
    token: key for key in NAMED_KEYS for token in key.tokens
}


def _modifier_pattern(modifiers: tuple[Modifier, ...]) -> str:
    """Build the accepted modifier grammar from the encoding table."""
    return "(?:" + "|".join(
        re.escape(alias) if len(alias) == 1 else f"(?i:{re.escape(alias)})"
        for modifier in modifiers for alias in modifier.spellings
    ) + ")"


_CSI_MODIFIERS = tuple(m for m in MODIFIERS if m.csi_u_value is not None)
_MODIFIER_BY_ALIAS = {
    alias.lower(): modifier for modifier in MODIFIERS for alias in modifier.spellings
}
_TMUX_CHORD = re.compile(
    "^(?:" + "|".join(m.tmux for m in MODIFIERS if m.tmux)
    + r")-(?:[A-Za-z0-9]|\\(?:\\||\[|\]|/))$"
)
_SHIFT_TAB = re.compile(
    "^" + _modifier_pattern(tuple(m for m in MODIFIERS if m.name == "shift"))
    + r"[+-](?i:tab)$"
)
_MODIFIED_KEY = re.compile(
    "^(?:" + _modifier_pattern(_CSI_MODIFIERS)
    + r"[+-])+(?P<key>[A-Za-z0-9]+)$"
)
_VALIDATION_CHORD = re.compile(
    "^(?P<prefix>(?:" + _modifier_pattern(MODIFIERS)
    + r"[+-])+)(?P<base>.*)$"
)


def find_key(token: str) -> NamedKey | None:
    """Return the table entry for a named-key token, or None."""
    return _BY_TOKEN.get(token)


def tmux_name(token: str) -> str | None:
    """Return the tmux `send-keys` name for `token`, or None for literal text."""
    key = find_key(token)
    if key is not None:
        if key.backtab:
            return None
        return key.tmux
    if _TMUX_CHORD.fullmatch(token):
        return token
    return None


def csi_u_bytes(token: str) -> list[str] | None:
    """Hex bytes for a modifier+key token tmux cannot encode.

    Returns one hex string per byte for `send-keys -H`, or `None` when the
    token carries no modifier, carries Alt/Meta (kept on tmux named keys),
    names a shifted Tab (sent as terminal backtab), or names a key with no
    verified CSI-u encoding (single characters with Ctrl alone, F-keys).
    """
    if _SHIFT_TAB.fullmatch(token):
        return None
    match = _MODIFIED_KEY.fullmatch(token)
    if match is None:
        return None
    modifiers = [
        _MODIFIER_BY_ALIAS[name.lower()]
        for name in re.findall(r"[A-Za-z]+", token[: -len(match.group("key"))])
    ]
    if len({modifier.name for modifier in modifiers}) != len(modifiers):
        return None
    value = 1 + sum(modifier.csi_u_value for modifier in modifiers)
    raw_key = match.group("key")
    key = find_key(raw_key)
    if key is not None and not key.backtab and key.codepoint is not None:
        sequence = f"\x1b[{key.codepoint};{value}u"
    elif key is not None and not key.backtab and key.final is not None:
        sequence = f"\x1b[1;{value}{key.final}"
    elif key is not None and not key.backtab and key.tilde is not None:
        sequence = f"\x1b[{key.tilde};{value}~"
    elif (
        value == 6
        and len(raw_key) == 1
        and raw_key.isascii()
        and raw_key.isalnum()
    ):
        sequence = f"\x1b[{ord(raw_key)};{value}u"
    else:
        return None
    return [f"{byte:02x}" for byte in sequence.encode("ascii")]


def encode(token: str) -> Encoding:
    """Classify one harness token as tmux, backtab, CSI-u or literal text."""
    if _SHIFT_TAB.fullmatch(token) or (
        (key := find_key(token)) is not None and key.backtab
    ):
        return Encoding(token, "backtab", next(k.tmux for k in NAMED_KEYS if k.backtab))
    raw = csi_u_bytes(token)
    if raw is not None:
        return Encoding(token, "csi-u", tuple(raw))
    name = tmux_name(token)
    if name is not None:
        return Encoding(token, "tmux", name)
    return Encoding(token, "literal", token)


def describe(encoding: Encoding) -> str:
    """A one-line description of how `encoding` is sent."""
    if encoding.kind == "tmux":
        return f"tmux key {encoding.value}"
    if encoding.kind == "csi-u":
        return "CSI-u bytes " + " ".join(encoding.value)
    if encoding.kind == "backtab":
        return "terminal backtab (BTab)"
    return f"literal text {encoding.value!r}"


def _known_spellings() -> list[str]:
    """Every accepted named-key token, excluding VSCode-only spellings."""
    spellings: list[str] = []
    for key in NAMED_KEYS:
        spellings.extend(key.tokens)
    return spellings


def _suggest(token: str) -> str | None:
    """The nearest named-key spelling, preferring a case-only correction."""
    spellings = _known_spellings()
    lower = token.lower()
    for spelling in spellings:
        if spelling.lower() == lower:
            return spelling
    matches = get_close_matches(token, spellings, n=1, cutoff=0.75)
    return matches[0] if matches else None


def _short_modifiers(prefix: str) -> list[str]:
    """Normalize a chord prefix such as `ctrl+S-` to `C-S`."""
    names = re.findall(r"[A-Za-z]+", prefix)
    return [
        _MODIFIER_BY_ALIAS[name.lower()].tmux or name for name in names
    ]


def _chord_suggestion(prefix: str, base: str) -> str | None:
    """The nearest encodable spelling of a modifier chord, if there is one."""
    modifiers = _short_modifiers(prefix)
    candidates: list[str] = []
    key = find_key(base)
    bases = key.tokens if key is not None else (base,)
    if key is None:
        nearest = _suggest(base)
        if nearest is not None:
            found = find_key(nearest)
            bases = found.tokens if found is not None else (nearest,)
    for candidate_base in bases:
        for separator in ("-", "+"):
            candidate = "".join(
                f"{modifier}{separator}" for modifier in modifiers
            ) + candidate_base
            if candidate not in candidates:
                candidates.append(candidate)
    for candidate in candidates:
        if encode(candidate).kind != "literal":
            return candidate
    return None


def validate(token: str) -> str | None:
    """Return a readable problem for `token`, or None when it can be sent.

    Check mode accepts named keys, supported chords and single characters.
    Unlike the send command, it rejects arbitrary multi-character text so an
    unknown key name cannot silently pass as literal text.
    """
    if not token:
        return "empty key token"
    if encode(token).kind != "literal":
        return None
    chord = _VALIDATION_CHORD.fullmatch(token)
    if chord is not None:
        prefix = chord.group("prefix")
        base = chord.group("base")
        if not base:
            return "modifier chord has no key"
        suggestion = _chord_suggestion(prefix, base)
        if suggestion is not None:
            return f"unsupported key `{token}`; did you mean `{suggestion}`?"
        return (
            f"unsupported key `{token}`; the harness has no terminal "
            "encoding for that modifier combination"
        )
    if len(token) == 1:
        return None
    suggestion = _suggest(token)
    if suggestion is None:
        suggestion = get_close_matches(token, _known_spellings(), n=1, cutoff=0)[0]
    return (
        f"unknown key `{token}`; nearest spelling is `{suggestion}`"
        "; send literal strings with `keys <text>` instead of `--check`"
    )


def _row(values: tuple[str, ...], widths: list[int]) -> str:
    """Join one table row with two-space columns and the given widths."""
    return "  ".join(value.ljust(widths[index]) for index, value in enumerate(values))


def render_list() -> str:
    """Render the key table for `scripts/ui keys --list` from the table data."""
    rows: list[tuple[str, ...]] = [
        ("KEY", "NEOVIM SPELLINGS", "VSCODE", "SENT AS")
    ]
    for key in NAMED_KEYS:
        if key.backtab:
            sent = "terminal backtab"
        elif key.codepoint or key.final or key.tilde:
            sent = f"tmux {key.tmux}; CSI-u when modified"
        else:
            sent = f"tmux {key.tmux}"
        rows.append((key.name, ", ".join(key.tokens), key.vscode, sent))
    widths = [
        max(len(row[index]) for row in rows) for index in range(len(rows[0]))
    ]
    lines = [_row(row, widths) for row in rows]
    modifiers = "; ".join(
        f"{modifier.name}={', '.join(modifier.spellings)}"
        for modifier in MODIFIERS
    )
    lines.append("")
    lines.append(f"Modifier aliases: {modifiers}.")
    lines.append(
        "Ctrl and Shift combine as `C-`, `S-` or `C-S-` before a key; Alt "
        "combines as `M-` for one character; Meta has no harness encoding."
    )
    lines.append(
        "Any other token is literal text, sent verbatim with `send-keys -l`."
    )
    lines.append(
        "Check mode accepts only named keys, supported chords and single "
        "characters; literal strings belong in `keys <text>`, not `--check`."
    )
    lines.append(
        "VSCODE is the canonical §3.8 keybindings spelling, not a token the "
        "Neovim harness accepts."
    )
    return "\n".join(lines)
