"""Scenario parser, predicate evaluator and runner (spec §A.5).

A scenario is a plain-text file with a small header, a `---` separator and one
statement per line. The runner turns each scenario into one real UI run per
size and profile, checks the statements against the live screen, state and
trace, and writes artefacts below `build/ui/<scenario>/`.

Nothing in this module sleeps: every wait polls the client's `debug.idle()`.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

from . import decoder, driver, fixtures
from .paths import BUILD_UI

DEFAULT_SIZE = "100x30"
SECONDARY_SIZE = "80x24"
DEFAULT_PROFILE = "standard"
SECONDARY_PROFILE = "vim"
STYLE_MODES = ("dim", "bold", "coloured", "plain")
WAIT_TIMEOUT_MS = 2000
SHELL_TIMEOUT_S = 30


class ScenarioError(RuntimeError):
    """A scenario file could not be parsed."""


class ShellStepError(ScenarioError):
    """A scenario shell statement failed, timed out or could not launch."""


@dataclass
class Expectation:
    """One `expect` predicate."""

    kind: str
    text: str | None = None
    pattern: str | None = None
    path: str | None = None
    value: Any = None
    mode: str | None = None
    key: str | None = None
    budget_ms: float | None = None
    operator: str = "<="
    name: str | None = None


@dataclass
class Statement:
    """One scenario statement: keys, wait, expect, capture or snapshot."""

    kind: str
    keys: list[str] = field(default_factory=list)
    expectation: Expectation | None = None
    name: str | None = None
    text: str | None = None
    profile: str | None = None
    command: str | None = None
    line: int = 0


@dataclass
class Scenario:
    """A parsed scenario file."""

    name: str
    path: Path
    fixture: str
    size: str | None = None
    profile: str | None = None
    size_only: bool = False
    profile_only: bool = False
    xfail: str | None = None
    # Extra environment variables for Neovim, e.g. MAGIT_LSP_PLACEMENT=vsplit.
    env: dict[str, str] = field(default_factory=dict)
    statements: list[Statement] = field(default_factory=list)


@dataclass
class ComboResult:
    """The outcome of one (size, profile) run of a scenario."""

    ok: bool
    reason: str = ""
    screen: str = ""
    profile: str = ""
    size: str = ""
    startup: bool = False


@dataclass
class Row:
    """One line of the result table."""

    scenario: str
    profile: str
    size: str
    result: str
    reason: str = ""


# ---------------------------------------------------------------------------
# Parsing


def strip_comment(line: str) -> str:
    """Remove a `#` comment, respecting quoted text and backslash escapes."""
    out: list[str] = []
    quote: str | None = None
    escaped = False
    for char in line:
        if escaped:
            out.append(char)
            escaped = False
            continue
        if char == "\\":
            out.append(char)
            escaped = True
            continue
        if quote is not None:
            out.append(char)
            if char == quote:
                quote = None
            continue
        if char in ("'", '"'):
            quote = char
            out.append(char)
            continue
        if char == "#":
            break
        out.append(char)
    return "".join(out).strip()


def parse_bool(value: str) -> bool:
    """Parse a header boolean."""
    text = value.strip().lower()
    if text in ("true", "yes", "on", "1"):
        return True
    if text in ("false", "no", "off", "0"):
        return False
    raise ScenarioError(f"invalid boolean `{value}`")


def unquote(text: str) -> str:
    """Strip one pair of surrounding quotes."""
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in ("'", '"'):
        return text[1:-1]
    return text


def parse_value(text: str) -> Any:
    """Parse an `==` right-hand side into a string, number or boolean."""
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in ("'", '"'):
        return text[1:-1]
    if text == "true":
        return True
    if text == "false":
        return False
    try:
        return int(text)
    except ValueError:
        pass
    try:
        return float(text)
    except ValueError:
        pass
    return text


def parse_keys(text: str) -> list[str]:
    """Split a `keys:` payload into individual key tokens."""
    try:
        keys = shlex.split(text, comments=False)
    except ValueError as error:
        raise ScenarioError(f"invalid keys `{text}`: {error}") from error
    if not keys:
        raise ScenarioError("`keys:` needs at least one key")
    return keys


def parse_env(text: str) -> dict[str, str]:
    """Parse an `env:` header payload of space-separated `KEY=VALUE` pairs."""
    try:
        tokens = shlex.split(text, comments=False)
    except ValueError as error:
        raise ScenarioError(f"invalid env `{text}`: {error}") from error
    if not tokens:
        raise ScenarioError("`env:` needs at least one KEY=VALUE pair")
    env: dict[str, str] = {}
    for token in tokens:
        key, separator, value = token.partition("=")
        if not separator or not key:
            raise ScenarioError(f"invalid env entry `{token}`; expected KEY=VALUE")
        env[key] = value
    return env


def parse_expectation(line: str) -> Expectation:
    """Parse one `expect` statement."""
    match = re.match(r"^expect\s+screen\s+contains\s+(.+)$", line)
    if match:
        return Expectation(kind="screen-contains", text=unquote(match.group(1)))

    match = re.match(r"^expect\s+screen\s+not-contains\s+(.+)$", line)
    if match:
        return Expectation(kind="screen-not-contains", text=unquote(match.group(1)))

    match = re.match(r"^expect\s+screen\s+matches\s+/(.*)/$", line)
    if match:
        return Expectation(kind="screen-matches", pattern=match.group(1))

    match = re.match(
        r"^expect\s+document\s+(\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')\s+"
        r"unchanged$",
        line,
    )
    if match:
        return Expectation(kind="document-unchanged", name=unquote(match.group(1)))

    match = re.match(r"^expect\s+no_message\s+/(.*)/$", line)
    if match:
        return Expectation(kind="no-message", pattern=match.group(1))

    match = re.match(
        r"^expect\s+style\s+(\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')\s+"
        r"(dim|bold|coloured|plain)$",
        line,
    )
    if match:
        return Expectation(
            kind="style", text=unquote(match.group(1)), mode=match.group(2)
        )

    match = re.match(
        r"^expect\s+cursor\s+on\s+(\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')$",
        line,
    )
    if match:
        return Expectation(kind="cursor", text=unquote(match.group(1)))

    match = re.match(
        r"^expect\s+latency\(\"([^\"]+)\"\)\s*(<=|<)\s*([0-9]+(?:\.[0-9]+)?)ms$",
        line,
    )
    if match:
        return Expectation(
            kind="latency",
            key=match.group(1),
            operator=match.group(2),
            budget_ms=float(match.group(3)),
        )

    match = re.match(r"^expect\s+([A-Za-z0-9_.]+)\s*==\s*(.+)$", line)
    if match:
        return Expectation(
            kind="path", path=match.group(1), value=parse_value(match.group(2))
        )

    raise ScenarioError(f"unknown expectation `{line}`")


def parse_statement_name(line: str, keyword: str, where: str) -> str:
    """Read and validate the artefact name after `keyword:`."""
    name = line[len(keyword) :].strip()
    if not name:
        raise ScenarioError(f"{where}: `{keyword}` needs a name")
    if "/" in name or "\\" in name or name in (".", ".."):
        raise ScenarioError(f"{where}: invalid {keyword} name `{name}`")
    return name


def parse_statement(line: str, where: str) -> Statement:
    """Parse one non-empty scenario line."""
    match = re.match(r"^keys\s+(standard|vim)\s*:\s*(.+)$", line)
    if match:
        return Statement(
            kind="keys",
            keys=parse_keys(match.group(2)),
            profile=match.group(1),
        )
    if line.startswith("keys:"):
        return Statement(kind="keys", keys=parse_keys(line[len("keys:") :]))
    if line == "wait":
        return Statement(kind="wait")
    if line == "wait prompt":
        return Statement(kind="wait-prompt")
    match = re.match(r"^wait\s+screen\s+contains\s+(.+)$", line)
    if match:
        text = unquote(match.group(1))
        if not text:
            raise ScenarioError(f"{where}: `wait screen contains` needs text")
        return Statement(kind="wait-screen", text=text)
    if line.startswith("snapshot:"):
        return Statement(
            kind="snapshot",
            name=parse_statement_name(line, "snapshot:", where),
        )
    if line.startswith("document:"):
        return Statement(
            kind="document",
            name=parse_statement_name(line, "document:", where),
        )
    if line.startswith("sh:"):
        command = line[len("sh:") :].strip()
        if not command:
            raise ScenarioError(f"{where}: `sh:` needs a command")
        return Statement(kind="sh", command=command)
    match = re.match(
        r"^goto\s+(\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')$", line
    )
    if match:
        text = unquote(match.group(1))
        if not text:
            raise ScenarioError(f"{where}: `goto` needs text")
        return Statement(kind="goto", text=text)
    if re.match(r"^goto\b", line):
        raise ScenarioError(f"{where}: `goto` needs quoted text")
    if line.startswith("expect "):
        return Statement(kind="expect", expectation=parse_expectation(line))
    raise ScenarioError(f"{where}: unknown statement `{line}`")


def parse_scenario(path: Path) -> Scenario:
    """Parse `path` into a `Scenario`."""
    path = Path(path)
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise ScenarioError(f"cannot read {path}: {error}") from error

    header: dict[str, str] = {}
    statements: list[Statement] = []
    in_body = False
    seen_separator = False

    for number, raw in enumerate(text.splitlines(), start=1):
        line = strip_comment(raw)
        if not line:
            continue
        where = f"{path.name}:{number}"
        if not in_body:
            if line == "---":
                in_body = True
                seen_separator = True
                continue
            if ":" not in line:
                raise ScenarioError(f"{where}: header line needs `key: value`")
            key, value = line.split(":", 1)
            key = key.strip()
            value = value.strip()
            if key in header:
                raise ScenarioError(f"{where}: duplicate header `{key}`")
            if key not in (
                "fixture",
                "size",
                "profile",
                "size-only",
                "profile-only",
                "xfail",
                "env",
            ):
                raise ScenarioError(f"{where}: unknown header `{key}`")
            header[key] = value
        else:
            statement = parse_statement(line, where)
            statement.line = number
            statements.append(statement)

    if not seen_separator:
        raise ScenarioError(f"{path.name}: missing `---` separator")
    fixture = header.get("fixture", "").strip()
    if not fixture:
        raise ScenarioError(f"{path.name}: missing `fixture` header")

    scenario = Scenario(
        name=path.stem,
        path=path,
        fixture=fixture,
        size=header.get("size") or None,
        profile=header.get("profile") or None,
        size_only=parse_bool(header["size-only"]) if "size-only" in header else False,
        profile_only=parse_bool(header["profile-only"])
        if "profile-only" in header
        else False,
        xfail=header.get("xfail") or None,
        env=parse_env(header["env"]) if "env" in header else {},
        statements=statements,
    )
    if scenario.size:
        driver.parse_size(scenario.size)
    if scenario.profile and scenario.profile not in ("standard", "vim"):
        raise ScenarioError(
            f"{path.name}: unknown profile `{scenario.profile}`; expected standard or vim"
        )
    return scenario


# ---------------------------------------------------------------------------
# Predicates


def _format_value(value: Any) -> str:
    return json.dumps(value)


def resolve_path(path: str, snapshot: dict) -> Any:
    """Resolve a snapshot path, including the aggregate selectors of §A.5."""
    parts = path.split(".")
    if parts == ["windows", "floating"]:
        return sum(1 for window in snapshot.get("windows", []) if window.get("float"))
    if parts in (["windows", "count"], ["windows", "length"]):
        return len(snapshot.get("windows", []))
    if parts == ["view_stack", "menus"]:
        return sum(
            1
            for view in snapshot.get("view_stack", [])
            if str(view.get("kind", "")).startswith("menu")
        )
    if parts in (["view_stack", "count"], ["view_stack", "length"]):
        return len(snapshot.get("view_stack", []))

    current: Any = snapshot
    for part in parts:
        if isinstance(current, dict):
            if part not in current:
                raise KeyError(path)
            current = current[part]
        elif isinstance(current, list):
            if part in ("length", "count"):
                current = len(current)
            elif part.isdigit() and int(part) < len(current):
                current = current[int(part)]
            else:
                raise KeyError(path)
        else:
            raise KeyError(path)
    return current


def check_screen_contains(screen: str, text: str) -> tuple[bool, str]:
    """True when `text` appears verbatim on the captured screen."""
    if text in screen:
        return True, f"screen contains {text!r}"
    return False, f"screen does not contain {text!r}"


def check_screen_not_contains(screen: str, text: str) -> tuple[bool, str]:
    """True when `text` is absent from the captured screen."""
    if text not in screen:
        return True, f"screen does not contain {text!r}"
    line = next((line for line in screen.splitlines() if text in line), "")
    return False, f"screen contains {text!r}: {line!r}"


def check_screen_matches(screen: str, pattern: str) -> tuple[bool, str]:
    """True when `pattern` matches somewhere on the captured screen."""
    try:
        match = re.search(pattern, screen, re.MULTILINE)
    except re.error as error:
        return False, f"invalid regex /{pattern}/: {error}"
    if match:
        return True, f"screen matches /{pattern}/: {match.group(0)!r}"
    return False, f"screen does not match /{pattern}/"


def _row_view(runs: Iterable[dict], row: int) -> tuple[str, list[dict]]:
    row_runs = [run for run in runs if run["row"] == row]
    if not row_runs:
        return "", []
    width = max(run["col"] + len(run["text"]) for run in row_runs)
    chars = [" "] * width
    for run in row_runs:
        for offset, char in enumerate(run["text"]):
            position = run["col"] + offset
            if position < width:
                chars[position] = char
    return "".join(chars), row_runs


def _overlapping_runs(
    runs: Iterable[dict], text: str
) -> tuple[list[dict], str | None]:
    """Runs that overlap `text`, plus the row line when found.

    Text such as `(--all)` is split by the renderer into a plain prefix, a
    styled flag and a plain suffix, so a style predicate has to look at every
    run the text spans, not at a single run.
    """
    rows = sorted({run["row"] for run in runs})
    for row in rows:
        line, row_runs = _row_view(runs, row)
        start = line.find(text)
        if start == -1:
            continue
        end = start + len(text)
        overlapping = [
            run
            for run in row_runs
            if run["col"] < end and run["col"] + len(run["text"]) > start
        ]
        return overlapping, line
    return [], None


def check_style(ansi: str, text: str, mode: str) -> tuple[bool, str]:
    """Check the style of `text` using the ANSI decoder."""
    if mode not in STYLE_MODES:
        return False, f"unknown style mode `{mode}`"
    runs = decoder.decode(ansi)
    overlapping, line = _overlapping_runs(runs, text)
    if not overlapping:
        return False, f"{text!r} is not on screen"
    if mode == "dim":
        ok = any(run["dim"] for run in overlapping)
    elif mode == "bold":
        ok = any(run["bold"] for run in overlapping)
    elif mode == "coloured":
        ok = any(run["fg"] is not None and not run["dim"] for run in overlapping)
    else:  # plain
        ok = all(
            run["fg"] is None and not run["dim"] and not run["bold"]
            for run in overlapping
        )
    actual = ", ".join(
        f"{run['text']!r} fg={run['fg']} dim={run['dim']} bold={run['bold']}"
        for run in overlapping
    )
    return ok, f"style of {text!r} is {mode}={ok} ({actual})"


def check_no_message(snapshot: dict, pattern: str) -> tuple[bool, str]:
    """True when no `:messages` line or notification matches `pattern`."""
    try:
        regex = re.compile(pattern)
    except re.error as error:
        return False, f"invalid regex /{pattern}/: {error}"
    lines = list(snapshot.get("messages") or []) + list(
        snapshot.get("notifications") or []
    )
    matches = [line for line in lines if regex.search(str(line))]
    if not matches:
        return True, f"no message matches /{pattern}/"
    return False, f"messages match /{pattern}/: {matches}"


def check_cursor(snapshot: dict, screen: str, text: str) -> tuple[bool, str]:
    """True when the cursor sits inside `text` on the captured screen."""
    cursor = snapshot.get("cursor") or {}
    row = int(cursor.get("row", -1))
    col = int(cursor.get("col", -1))
    lines = screen.splitlines()
    if row < 0 or row >= len(lines):
        return False, f"cursor row {row} is outside the screen"
    line = lines[row]
    starts = [index for index in range(len(line)) if line.startswith(text, index)]
    if not starts:
        return False, f"cursor row {row} does not contain {text!r}: {line!r}"
    ok = any(start <= col <= start + len(text) for start in starts)
    return ok, f"cursor ({row},{col}) line {line!r}"


def measure_latency(key: str, events: list[dict]) -> float | None:
    """Key-to-idle milliseconds for the last press of `key`, from the trace."""
    groups: list[list[dict]] = []
    for event in events:
        if not groups or groups[-1][0].get("id") != event.get("id"):
            groups.append([event])
        else:
            groups[-1].append(event)
    for group in reversed(groups):
        pressed = next(
            (event for event in group if event.get("event") == "key_pressed"), None
        )
        if pressed is None:
            continue
        steps = (pressed.get("detail") or {}).get("steps") or []
        if key != " ".join(steps) and key not in steps:
            continue
        start = float(pressed.get("t_ms", 0.0))
        end = start
        for event in group:
            if event.get("event") == "idle":
                end = float(event.get("t_ms", start))
        if end == start:
            end = float(group[-1].get("t_ms", start))
        return end - start
    return None


def budget_scale() -> float:
    """The `UI_BUDGET_SCALE` multiplier, defaulting to 1."""
    value = os.environ.get("UI_BUDGET_SCALE", "").strip()
    if not value:
        return 1.0
    try:
        return float(value)
    except ValueError:
        return 1.0


def strict_latency() -> bool:
    """True when `UI_STRICT_LATENCY=1` turns an over-budget latency into a fail."""
    return os.environ.get("UI_STRICT_LATENCY", "").strip() == "1"


def check_latency(
    events: list[dict], key: str, operator: str, budget_ms: float
) -> tuple[bool, str]:
    """Check a key-to-idle budget, scaled by `UI_BUDGET_SCALE`.

    The measured value is always reported. An over-budget value only fails the
    run when `UI_STRICT_LATENCY=1`; otherwise it is informational so a loaded
    machine does not turn a latency regression guard into a flaky failure.
    """
    actual = measure_latency(key, events)
    budget = budget_ms * budget_scale()
    if actual is None:
        return False, f"no trace for key {key!r}"
    within = actual <= budget if operator == "<=" else actual < budget
    if within:
        return True, f"latency({key!r}) = {actual:.1f}ms (budget {budget:.1f}ms)"
    strict = strict_latency()
    note = "" if strict else " (informational; set UI_STRICT_LATENCY=1 to fail)"
    return (
        not strict,
        f"latency({key!r}) = {actual:.1f}ms over budget {budget:.1f}ms{note}",
    )


def format_expectation(expectation: Expectation) -> str:
    """Render an expectation back into DSL syntax for reports."""
    if expectation.kind == "screen-contains":
        return f'expect screen contains "{expectation.text}"'
    if expectation.kind == "screen-not-contains":
        return f'expect screen not-contains "{expectation.text}"'
    if expectation.kind == "screen-matches":
        return f"expect screen matches /{expectation.pattern}/"
    if expectation.kind == "document-unchanged":
        return f'expect document "{expectation.name}" unchanged'
    if expectation.kind == "no-message":
        return f"expect no_message /{expectation.pattern}/"
    if expectation.kind == "style":
        return f'expect style "{expectation.text}" {expectation.mode}'
    if expectation.kind == "cursor":
        return f'expect cursor on "{expectation.text}"'
    if expectation.kind == "latency":
        return (
            f'expect latency("{expectation.key}") '
            f"{expectation.operator} {expectation.budget_ms:g}ms"
        )
    return f"expect {expectation.path} == {_format_value(expectation.value)}"


def check_document_unchanged(
    name: str, document: str | None, documents: dict[str, str]
) -> tuple[bool, str]:
    """True when `document` still equals the text captured as `name`."""
    saved = documents.get(name)
    if saved is None:
        return False, f"no captured document {name!r}"
    if document is None:
        return False, f"no current document to compare with {name!r}"
    if document == saved:
        return True, f"document {name!r} is unchanged"
    return False, f"document {name!r} changed"


def evaluate(
    expectation: Expectation,
    *,
    screen: str,
    ansi: str,
    snapshot: dict,
    events: list[dict],
    document: str | None = None,
    documents: dict[str, str] | None = None,
) -> tuple[bool, str]:
    """Evaluate one expectation against a captured UI state."""
    if expectation.kind == "document-unchanged":
        return check_document_unchanged(
            expectation.name or "", document, documents or {}
        )
    if expectation.kind == "screen-contains":
        return check_screen_contains(screen, expectation.text or "")
    if expectation.kind == "screen-not-contains":
        return check_screen_not_contains(screen, expectation.text or "")
    if expectation.kind == "screen-matches":
        return check_screen_matches(screen, expectation.pattern or "")
    if expectation.kind == "no-message":
        return check_no_message(snapshot, expectation.pattern or "")
    if expectation.kind == "style":
        return check_style(ansi, expectation.text or "", expectation.mode or "")
    if expectation.kind == "cursor":
        return check_cursor(snapshot, screen, expectation.text or "")
    if expectation.kind == "latency":
        return check_latency(
            events,
            expectation.key or "",
            expectation.operator,
            expectation.budget_ms or 0.0,
        )
    if expectation.kind == "path":
        try:
            actual = resolve_path(expectation.path or "", snapshot)
        except KeyError:
            return False, f"snapshot has no path {expectation.path!r}"
        return (
            actual == expectation.value,
            f"{expectation.path} = {_format_value(actual)}",
        )
    return False, f"unknown expectation kind {expectation.kind!r}"


# ---------------------------------------------------------------------------
# Runner


def combinations(scenario: Scenario) -> list[tuple[str, str]]:
    """Every (size, profile) pair the scenario runs.

    The header value comes first; unless `size-only`/`profile-only` is set, the
    other default is added so every scenario still runs at 100x30 and 80x24
    under both profiles.
    """
    sizes = [scenario.size or DEFAULT_SIZE]
    if not scenario.size_only:
        other = SECONDARY_SIZE if sizes[0] != SECONDARY_SIZE else DEFAULT_SIZE
        if other not in sizes:
            sizes.append(other)
    profiles = [scenario.profile or DEFAULT_PROFILE]
    if not scenario.profile_only:
        other = (
            SECONDARY_PROFILE
            if profiles[0] != SECONDARY_PROFILE
            else DEFAULT_PROFILE
        )
        if other not in profiles:
            profiles.append(other)
    return [(size, profile) for size in sizes for profile in profiles]


def classify(xfail: str | None, results: list[ComboResult]) -> str:
    """PASS / FAIL / XFAIL for a scenario from its combination results."""
    if any(result.startup for result in results):
        return "FAIL"
    if xfail:
        if any(not result.ok for result in results):
            return "XFAIL"
        return "FAIL"  # XPASS: the bug is fixed, so the tag must go.
    return "PASS" if all(result.ok for result in results) else "FAIL"


def _snapshot_paths(scenario: Scenario, profile: str, size: str, name: str) -> tuple[Path, Path]:
    base = BUILD_UI / scenario.name
    variant = base / f"{profile}-{size}"
    variant.mkdir(parents=True, exist_ok=True)
    base.mkdir(parents=True, exist_ok=True)
    return variant / name, base / name


def _write_snapshot(
    run: driver.Run,
    scenario: Scenario,
    profile: str,
    size: str,
    name: str,
    snapshots: list[tuple[str, str, str, str, str]],
) -> None:
    screen = driver.capture(run)
    ansi = driver.capture(run, ansi=True)
    variant, canonical = _snapshot_paths(scenario, profile, size, name)
    for stem in (variant, canonical):
        Path(f"{stem}.screen").write_text(screen + "\n", encoding="utf-8")
        Path(f"{stem}.ansi").write_text(ansi, encoding="utf-8")
    snapshots.append((scenario.name, profile, size, name, screen))


def _clear_command_line(run: driver.Run) -> None:
    """Dismiss the W10 message the readonly workaround records at startup.

    The message stays in `:messages` (which `no_message` reads); this only
    keeps the captured screen free of a command-line artefact every run shares.
    """
    try:
        driver.clear_message_line(run)
    except driver.DriverError:
        pass


def _run_shell(run: driver.Run, command: str, where: int) -> None:
    """Run `command` in the fixture repo with the deterministic Git env.

    Scenarios use this to change the repository outside Neovim (the
    auto-refresh watcher test modifies a tracked file this way). Failures carry
    their statement and command so one combination can fail without aborting
    later profile/size combinations.
    """
    try:
        result = subprocess.run(
            command,
            shell=True,
            cwd=run.repo_path,
            env=fixtures.determinism_env(run.home_path),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=SHELL_TIMEOUT_S,
        )
    except subprocess.TimeoutExpired as error:
        detail = _shell_output(error.stderr or error.stdout)
        suffix = f": {detail}" if detail else ""
        raise ShellStepError(
            f"statement {where}: `sh: {command}` timed out after "
            f"{SHELL_TIMEOUT_S}s{suffix}"
        ) from error
    except OSError as error:
        raise ShellStepError(
            f"statement {where}: could not run `sh: {command}`: {error}"
        ) from error
    if result.returncode != 0:
        detail = _shell_output(result.stderr or result.stdout)
        suffix = f": {detail}" if detail else ""
        raise ShellStepError(
            f"statement {where}: `sh: {command}` failed "
            f"(exit {result.returncode}){suffix}"
        )


def _shell_output(output: str | bytes | None) -> str:
    """Normalize and cap untrusted shell output in a failed-step report."""
    if isinstance(output, bytes):
        output = output.decode("utf-8", errors="replace")
    detail = " ".join((output or "").split())
    return detail if len(detail) <= 1000 else detail[:997] + "..."


def _run_combo(
    scenario: Scenario,
    size: str,
    profile: str,
    snapshots: list[tuple[str, str, str, str, str]],
    verbose: bool = False,
) -> ComboResult:
    try:
        run = driver.start(
            scenario.fixture, size=size, profile=profile, extra_env=scenario.env
        )
    except driver.StartupError as error:
        return ComboResult(
            ok=False,
            reason=f"startup: {error}",
            screen="",
            profile=profile,
            size=size,
            startup=True,
        )
    except driver.DriverError as error:
        return ComboResult(
            ok=False, reason=str(error), screen="", profile=profile, size=size
        )

    documents: dict[str, str] = {}
    try:
        _clear_command_line(run)
        for number, statement in enumerate(scenario.statements, start=1):
            if statement.kind == "keys":
                if statement.profile and statement.profile != profile:
                    continue
                driver.send_keys(run, statement.keys)
            elif statement.kind == "wait":
                if not driver.wait_idle(run, timeout_ms=WAIT_TIMEOUT_MS):
                    return ComboResult(
                        ok=False,
                        reason="wait timed out",
                        screen=driver.capture(run),
                        profile=profile,
                        size=size,
                    )
            elif statement.kind == "wait-prompt":
                if not driver.wait_prompt(run, timeout_ms=WAIT_TIMEOUT_MS):
                    return ComboResult(
                        ok=False,
                        reason="wait timed out for a prompt",
                        screen=driver.capture(run),
                        profile=profile,
                        size=size,
                    )
            elif statement.kind == "wait-screen":
                if not driver.wait_screen(
                    run, statement.text or "", timeout_ms=WAIT_TIMEOUT_MS
                ):
                    return ComboResult(
                        ok=False,
                        reason=(
                            f"wait timed out for screen text {statement.text!r}"
                        ),
                        screen=driver.capture(run),
                        profile=profile,
                        size=size,
                    )
            elif statement.kind == "document":
                documents[statement.name or ""] = driver.document(run)
            elif statement.kind == "sh":
                try:
                    _run_shell(run, statement.command or "", where=number)
                except ShellStepError as error:
                    try:
                        screen = driver.capture(run)
                    except driver.DriverError as capture_error:
                        screen = f"(screen unavailable: {capture_error})"
                    return ComboResult(
                        ok=False,
                        reason=str(error),
                        screen=screen,
                        profile=profile,
                        size=size,
                    )
            elif statement.kind == "goto":
                try:
                    driver.goto_text(run, statement.text or "")
                except driver.DriverError as error:
                    return ComboResult(
                        ok=False,
                        reason=f"goto {statement.text!r}: {error}",
                        screen=driver.capture(run),
                        profile=profile,
                        size=size,
                    )
            elif statement.kind == "snapshot":
                _write_snapshot(
                    run, scenario, profile, size, statement.name or "snapshot", snapshots
                )
            elif statement.kind == "expect":
                expectation = statement.expectation
                assert expectation is not None
                try:
                    ok, actual = evaluate(
                        expectation,
                        screen=driver.capture(run),
                        ansi=driver.capture(run, ansi=True),
                        snapshot=driver.snapshot(run),
                        events=driver.read_trace(run),
                        document=(
                            driver.document(run)
                            if expectation.kind == "document-unchanged"
                            else None
                        ),
                        documents=documents,
                    )
                except driver.DriverError as error:
                    ok, actual = False, str(error)
                if expectation.kind == "latency":
                    if ok and (verbose or "over budget" in actual):
                        print(
                            f"{scenario.name} [{profile} {size}] "
                            f"{format_expectation(expectation)}: {actual}"
                        )
                if not ok:
                    return ComboResult(
                        ok=False,
                        reason=(
                            f"{scenario.path.name}:{statement.line}: "
                            f"{format_expectation(expectation)}: {actual}"
                        ),
                        screen=driver.capture(run),
                        profile=profile,
                        size=size,
                    )
        return ComboResult(ok=True, profile=profile, size=size)
    except driver.DriverError as error:
        try:
            screen = driver.capture(run)
        except driver.DriverError:
            screen = ""
        return ComboResult(
            ok=False, reason=str(error), screen=screen, profile=profile, size=size
        )
    finally:
        driver.stop(run)


def _trim_screen(screen: str) -> str:
    """Trim right whitespace from rows and discard trailing blank rows."""
    rows = [row.rstrip() for row in screen.splitlines()]
    while rows and not rows[-1]:
        rows.pop()
    return "\n".join(rows)


def _save_failure_screen(scenario: Scenario, result: ComboResult) -> Path:
    """Save a failed combination's screen under its scenario artefacts."""
    path = BUILD_UI / scenario.name / f"{result.profile}-{result.size}" / "failure.screen"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(_trim_screen(result.screen) + "\n", encoding="utf-8")
    return path


def _report_failure(
    scenario: Scenario,
    result: ComboResult,
    show_screen: bool = True,
    show_reason: bool = True,
) -> None:
    label = f"{scenario.name} [{result.profile} {result.size}]"
    path = _save_failure_screen(scenario, result)
    if result.startup:
        print(f"--- {label} STARTUP FAILED (harness/socket, not scenario behaviour)")
        if show_reason:
            print(result.reason)
        print(f"screen artifact: {path}")
        print()
        return
    print(f"--- {label} FAILED")
    if result.reason and show_reason:
        print(f"expect: {result.reason}")
    if show_screen and result.screen:
        print("screen:")
        print(_trim_screen(result.screen))
    print(f"screen artifact: {path}")
    print()


def _print_table(rows: list[Row], verbose: bool = False) -> None:
    rows = [row for row in rows if verbose or row.result != "PASS"]
    if not rows:
        return
    headers = ("RESULT", "SCENARIO", "PROFILE", "SIZE", "REASON")
    widths = [len(header) for header in headers]
    for row in rows:
        values = (
            row.result,
            row.scenario,
            row.profile,
            row.size,
            (" ".join(str(row.reason).split()))[:100],
        )
        for index, value in enumerate(values):
            widths[index] = max(widths[index], len(str(value)))
    print("  ".join(header.ljust(widths[index]) for index, header in enumerate(headers)))
    for row in rows:
        values = (
            row.result,
            row.scenario,
            row.profile,
            row.size,
            (" ".join(str(row.reason).split()))[:100],
        )
        print("  ".join(str(value).ljust(widths[index]) for index, value in enumerate(values)))


def _write_markdown(path: Path, snapshots: list[tuple[str, str, str, str, str]]) -> None:
    lines = ["# UI tour", ""]
    group: tuple[str, str, str] | None = None
    for scenario, profile, size, name, screen in snapshots:
        if (scenario, profile, size) != group:
            group = (scenario, profile, size)
            lines.append(f"## {scenario} — {profile} {size}")
            lines.append("")
        lines.append(f"### {name}")
        lines.append("")
        lines.append("~~~")
        lines.append(screen)
        lines.append("~~~")
        lines.append("")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")


def expand_paths(paths: Iterable[str]) -> list[Path]:
    """Expand directories into their `.scn` files, keeping argument order."""
    expanded: list[Path] = []
    for value in paths:
        path = Path(value)
        if path.is_dir():
            expanded.extend(sorted(path.glob("*.scn")))
        else:
            expanded.append(path)
    return expanded


def run(paths: Iterable[str], markdown: str | None = None, verbose: bool = False) -> int:
    """Run every scenario and print the result table; return the exit code."""
    driver.check_tools()
    files = expand_paths(paths)
    if not files:
        print("no scenarios found", file=sys.stderr)
        return 2

    snapshots: list[tuple[str, str, str, str, str]] = []
    rows: list[Row] = []
    reported: set[str] = set()
    for path in files:
        try:
            scenario = parse_scenario(path)
        except ScenarioError as error:
            print(f"--- {path.name} FAILED: {error}")
            rows.append(Row(path.stem, "-", "-", "FAIL", str(error)))
            continue

        results: list[ComboResult] = []
        reported_reasons: set[str] = set()
        for size, profile in combinations(scenario):
            result = _run_combo(scenario, size, profile, snapshots, verbose=verbose)
            results.append(result)
            if not result.ok:
                _save_failure_screen(scenario, result)
                if not scenario.xfail or verbose or result.startup:
                    show_reason = result.reason not in reported_reasons
                    _report_failure(
                        scenario,
                        result,
                        show_screen=scenario.name not in reported,
                        show_reason=show_reason,
                    )
                    reported_reasons.add(result.reason)
                    reported.add(scenario.name)

        outcome = classify(scenario.xfail, results)
        failures = [result for result in results if not result.ok]
        if outcome == "FAIL" and scenario.xfail and all(result.ok for result in results):
            reason = "XPASS: remove the xfail tag"
            print(f"--- {scenario.name} FAILED: {reason}")
        elif failures and outcome != "XFAIL":
            first = failures[0]
            reason = f"[{first.profile} {first.size}] details above"
        else:
            reason = ""
        rows.append(
            Row(
                scenario.name,
                scenario.profile or "*",
                scenario.size or "*",
                outcome,
                reason,
            )
        )

    _print_table(rows, verbose=verbose)
    failures = sum(1 for row in rows if row.result == "FAIL")
    xfails = sum(1 for row in rows if row.result == "XFAIL")
    passes = sum(1 for row in rows if row.result == "PASS")
    if markdown:
        _write_markdown(Path(markdown), snapshots)
        print(f"tour written to {markdown}")

    print(f"Summary: {passes} PASS, {xfails} XFAIL, {failures} FAIL")

    return 1 if failures else 0
