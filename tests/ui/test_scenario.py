"""Unit tests for the scenario DSL and its predicates (§A.5, U0.5)."""

from __future__ import annotations

import contextlib
import io
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import driver, scenario  # noqa: E402


HEADER = """\
# a comment
fixture: modified-two-files
size: 100x30
profile: vim
size-only: true
profile-only: true
xfail: bug-2
env: MAGIT_LSP_PLACEMENT=vsplit
---
keys: m ?
wait
expect screen contains "Transient and dwim commands"
expect screen not-contains "Committing"
expect screen matches /Unstaged changes \\(\\d+\\)/
expect windows.floating == 1
expect view_stack.menus == 2
expect pending == "-"
expect pending == ""
expect no_message /W10|readonly/
expect style "(--all)" dim
expect cursor on "Unstaged changes"
expect latency("?") <= 100ms
snapshot: first
"""


def write_temp(text: str, name: str = "case.scn") -> Path:
    directory = tempfile.mkdtemp()
    path = Path(directory) / name
    path.write_text(text, encoding="utf-8")
    return path


def sample_snapshot() -> dict:
    return {
        "cursor": {"row": 1, "col": 4},
        "windows": [
            {"id": 1, "float": False, "filetype": "magit-lsp"},
            {"id": 2, "float": True, "filetype": "magit-lsp"},
        ],
        "view_stack": [
            {"id": "v1", "kind": "status"},
            {"id": "v2", "kind": "menu.commit"},
            {"id": "v3", "kind": "menu.dispatch"},
        ],
        "pending": "-",
        "messages": ["W10: Warning: Changing a readonly file"],
        "notifications": ["Committing…"],
    }


def sample_events() -> list[dict]:
    return [
        {"id": 0, "t_ms": 0.0, "event": "idle", "detail": {"views": 0}},
        {"id": 1, "t_ms": 10.0, "event": "key_pressed", "detail": {"steps": ["?"]}},
        {"id": 1, "t_ms": 12.0, "event": "key_sent", "detail": {"key": "?"}},
        {"id": 1, "t_ms": 30.0, "event": "idle", "detail": {"views": 1}},
    ]


class ParserTest(unittest.TestCase):
    def test_full_header_and_statements(self) -> None:
        parsed = scenario.parse_scenario(write_temp(HEADER))
        self.assertEqual(parsed.name, "case")
        self.assertEqual(parsed.fixture, "modified-two-files")
        self.assertEqual(parsed.size, "100x30")
        self.assertEqual(parsed.profile, "vim")
        self.assertTrue(parsed.size_only)
        self.assertTrue(parsed.profile_only)
        self.assertEqual(parsed.xfail, "bug-2")
        self.assertEqual(parsed.env, {"MAGIT_LSP_PLACEMENT": "vsplit"})
        self.assertEqual(
            [statement.kind for statement in parsed.statements],
            ["keys", "wait"] + ["expect"] * 11 + ["snapshot"],
        )
        self.assertEqual(parsed.statements[0].keys, ["m", "?"])
        self.assertEqual(parsed.statements[-1].name, "first")

    def test_defaults(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp("fixture: clean\n---\nwait\n", "defaults.scn")
        )
        self.assertIsNone(parsed.size)
        self.assertIsNone(parsed.profile)
        self.assertFalse(parsed.size_only)
        self.assertFalse(parsed.profile_only)
        self.assertIsNone(parsed.xfail)
        self.assertEqual(parsed.env, {})
        self.assertEqual(parsed.name, "defaults")

    def test_statement_retains_source_line(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp("fixture: clean\n---\n\nexpect screen contains 'x'\n", "line.scn")
        )
        self.assertEqual(parsed.statements[0].line, 4)

    def test_missing_fixture_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(write_temp("---\nwait\n"))

    def test_missing_separator_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(write_temp("fixture: clean\n"))

    def test_unknown_header_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(write_temp("fixture: clean\nbogus: 1\n---\n"))

    def test_invalid_env_entry_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\nenv: NOVALUE\n---\n")
            )

    def test_duplicate_header_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\nfixture: clean\n---\n")
            )

    def test_unknown_statement_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(write_temp("fixture: clean\n---\nsleep 1\n"))

    def test_unknown_profile_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\nprofile: emacs\n---\n")
            )

    def test_invalid_size_is_rejected(self) -> None:
        with self.assertRaises(driver.DriverError):
            scenario.parse_scenario(
                write_temp("fixture: clean\nsize: huge\n---\n")
            )

    def test_bad_boolean_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\nsize-only: maybe\n---\n")
            )

    def test_bad_expectation_is_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\n---\nexpect screen smells\n")
            )

    def test_bad_keys_are_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp('fixture: clean\n---\nkeys: "unterminated\n')
            )

    def test_snapshot_needs_a_name(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(write_temp("fixture: clean\n---\nsnapshot:\n"))

    def test_snapshot_rejects_paths(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\n---\nsnapshot: ../escape\n")
            )

    def test_profile_keys(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                "fixture: clean\n---\n"
                "keys standard: g\n"
                "keys vim: g r\n"
                'keys: x\n',
                "profile-keys.scn",
            )
        )
        self.assertEqual(parsed.statements[0].profile, "standard")
        self.assertEqual(parsed.statements[0].keys, ["g"])
        self.assertEqual(parsed.statements[1].profile, "vim")
        self.assertEqual(parsed.statements[1].keys, ["g", "r"])
        self.assertIsNone(parsed.statements[2].profile)

    def test_unknown_profile_keys_are_rejected(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\n---\nkeys emacs: g\n")
            )

    def test_wait_screen_contains(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                "fixture: clean\n---\n"
                'wait screen contains "Nothing staged. Stage all changes?"\n'
                "wait\n",
                "wait-screen.scn",
            )
        )
        self.assertEqual(parsed.statements[0].kind, "wait-screen")
        self.assertEqual(
            parsed.statements[0].text, "Nothing staged. Stage all changes?"
        )
        self.assertEqual(parsed.statements[1].kind, "wait")

    def test_wait_prompt(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp("fixture: clean\n---\nwait prompt\n", "wait-prompt.scn")
        )
        self.assertEqual(parsed.statements[0].kind, "wait-prompt")

    def test_wait_screen_contains_needs_text(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\n---\nwait screen contains\n")
            )

    def test_document_capture_and_expectation(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                "fixture: clean\n---\n"
                "document: before\n"
                "keys: J\n"
                'expect document "before" unchanged\n',
                "document.scn",
            )
        )
        self.assertEqual(parsed.statements[0].kind, "document")
        self.assertEqual(parsed.statements[0].name, "before")
        expectation = parsed.statements[2].expectation
        self.assertIsNotNone(expectation)
        self.assertEqual(expectation.kind, "document-unchanged")
        self.assertEqual(expectation.name, "before")

    def test_document_name_is_required(self) -> None:
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(write_temp("fixture: clean\n---\ndocument:\n"))
        with self.assertRaises(scenario.ScenarioError):
            scenario.parse_scenario(
                write_temp("fixture: clean\n---\ndocument: ../escape\n")
            )

    def test_goto_statement(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                'fixture: clean\n---\ngoto "+line3 CHANGED"\nwait\n',
                "goto.scn",
            )
        )
        self.assertEqual(parsed.statements[0].kind, "goto")
        self.assertEqual(parsed.statements[0].text, "+line3 CHANGED")
        self.assertEqual(parsed.statements[1].kind, "wait")

    def test_goto_accepts_single_quotes(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp("fixture: clean\n---\ngoto 'Unstaged changes'\n", "goto.scn")
        )
        self.assertEqual(parsed.statements[0].text, "Unstaged changes")

    def test_goto_needs_quoted_text(self) -> None:
        for line in ("goto\n", "goto line3\n"):
            with self.subTest(line=line):
                with self.assertRaises(scenario.ScenarioError):
                    scenario.parse_scenario(
                        write_temp(f"fixture: clean\n---\n{line}", "goto.scn")
                    )


class HelperTest(unittest.TestCase):
    def test_strip_comment_respects_quotes(self) -> None:
        self.assertEqual(scenario.strip_comment('expect "a # b" # comment'), 'expect "a # b"')
        self.assertEqual(scenario.strip_comment("plain # comment"), "plain")

    def test_parse_keys(self) -> None:
        self.assertEqual(scenario.parse_keys("m ?"), ["m", "?"])
        self.assertEqual(scenario.parse_keys("c c"), ["c", "c"])
        self.assertEqual(scenario.parse_keys("- a"), ["-", "a"])
        self.assertEqual(scenario.parse_keys('"Space" x'), ["Space", "x"])

    def test_parse_value(self) -> None:
        self.assertEqual(scenario.parse_value("1"), 1)
        self.assertEqual(scenario.parse_value("0"), 0)
        self.assertEqual(scenario.parse_value("1.5"), 1.5)
        self.assertIs(scenario.parse_value("true"), True)
        self.assertIs(scenario.parse_value("false"), False)
        self.assertEqual(scenario.parse_value('"-"'), "-")
        self.assertEqual(scenario.parse_value('""'), "")
        self.assertEqual(scenario.parse_value("word"), "word")

    def test_unquote(self) -> None:
        self.assertEqual(scenario.unquote('"hello world"'), "hello world")
        self.assertEqual(scenario.unquote("'hello'"), "hello")
        self.assertEqual(scenario.unquote("plain"), "plain")

    def test_format_expectation_round_trip(self) -> None:
        cases = [
            'expect screen contains "text"',
            'expect screen not-contains "text"',
            "expect screen matches /a.*b/",
            "expect no_message /W10|readonly/",
            'expect style "(--all)" dim',
            'expect cursor on "Unstaged changes"',
            'expect latency("?") <= 100ms',
            "expect windows.floating == 1",
            'expect pending == "-"',
            'expect document "before" unchanged',
        ]
        for line in cases:
            with self.subTest(line=line):
                expectation = scenario.parse_expectation(line)
                self.assertEqual(scenario.format_expectation(expectation), line)


class ScreenPredicateTest(unittest.TestCase):
    def test_contains(self) -> None:
        self.assertEqual(
            scenario.check_screen_contains("a\nb\n", "b"), (True, "screen contains 'b'")
        )
        ok, actual = scenario.check_screen_contains("a\n", "b")
        self.assertFalse(ok)
        self.assertIn("does not contain", actual)

    def test_not_contains(self) -> None:
        self.assertTrue(scenario.check_screen_not_contains("a\n", "b")[0])
        ok, actual = scenario.check_screen_not_contains("a\nb\n", "b")
        self.assertFalse(ok)
        self.assertIn("'b'", actual)

    def test_matches(self) -> None:
        self.assertTrue(
            scenario.check_screen_matches("Unstaged changes (2)", r"Unstaged changes \(\d+\)")[0]
        )
        self.assertFalse(scenario.check_screen_matches("nothing", r"Unstaged")[0])
        ok, actual = scenario.check_screen_matches("x", "[")
        self.assertFalse(ok)
        self.assertIn("invalid regex", actual)

    def test_evaluate_screen_contains(self) -> None:
        expectation = scenario.parse_expectation('expect screen contains "yes"')
        ok, actual = scenario.evaluate(
            expectation, screen="yes", ansi="", snapshot={}, events=[]
        )
        self.assertTrue(ok)
        self.assertIn("yes", actual)


class PathPredicateTest(unittest.TestCase):
    def test_aggregate_paths(self) -> None:
        snapshot = sample_snapshot()
        self.assertEqual(scenario.resolve_path("windows.floating", snapshot), 1)
        self.assertEqual(scenario.resolve_path("windows.count", snapshot), 2)
        self.assertEqual(scenario.resolve_path("windows.length", snapshot), 2)
        self.assertEqual(scenario.resolve_path("view_stack.menus", snapshot), 2)
        self.assertEqual(scenario.resolve_path("view_stack.length", snapshot), 3)
        self.assertEqual(scenario.resolve_path("pending", snapshot), "-")
        self.assertEqual(scenario.resolve_path("windows.1.float", snapshot), True)

    def test_missing_path(self) -> None:
        with self.assertRaises(KeyError):
            scenario.resolve_path("windows.nope", sample_snapshot())
        with self.assertRaises(KeyError):
            scenario.resolve_path("pending.deep", sample_snapshot())

    def test_evaluate_path(self) -> None:
        snapshot = sample_snapshot()
        for line, expected in [
            ("expect windows.floating == 1", True),
            ("expect view_stack.menus == 2", True),
            ('expect pending == "-"', True),
            ('expect pending == ""', False),
        ]:
            with self.subTest(line=line):
                ok, _ = scenario.evaluate(
                    scenario.parse_expectation(line),
                    screen="",
                    ansi="",
                    snapshot=snapshot,
                    events=[],
                )
                self.assertEqual(ok, expected)

    def test_evaluate_missing_path_reports_actual(self) -> None:
        ok, actual = scenario.evaluate(
            scenario.parse_expectation("expect view_stack.nope == 1"),
            screen="",
            ansi="",
            snapshot=sample_snapshot(),
            events=[],
        )
        self.assertFalse(ok)
        self.assertIn("no path", actual)


class DocumentPredicateTest(unittest.TestCase):
    def test_unchanged(self) -> None:
        ok, actual = scenario.check_document_unchanged(
            "before", "one\ntwo", {"before": "one\ntwo"}
        )
        self.assertTrue(ok)
        self.assertIn("unchanged", actual)

    def test_changed(self) -> None:
        ok, actual = scenario.check_document_unchanged(
            "before", "one\ntwo three", {"before": "one\ntwo"}
        )
        self.assertFalse(ok)
        self.assertIn("changed", actual)

    def test_missing_capture(self) -> None:
        ok, actual = scenario.check_document_unchanged("before", "one", {})
        self.assertFalse(ok)
        self.assertIn("no captured document", actual)

    def test_missing_current_document(self) -> None:
        ok, actual = scenario.check_document_unchanged(
            "before", None, {"before": "one"}
        )
        self.assertFalse(ok)
        self.assertIn("no current document", actual)

    def test_evaluate_document(self) -> None:
        expectation = scenario.parse_expectation(
            'expect document "before" unchanged'
        )
        ok, _ = scenario.evaluate(
            expectation,
            screen="",
            ansi="",
            snapshot={},
            events=[],
            document="same",
            documents={"before": "same"},
        )
        self.assertTrue(ok)


class MessagePredicateTest(unittest.TestCase):
    def test_no_match(self) -> None:
        ok, _ = scenario.check_no_message(sample_snapshot(), r"W10|readonly")
        self.assertFalse(ok)

    def test_match_absent(self) -> None:
        snapshot = {"messages": [], "notifications": []}
        self.assertTrue(scenario.check_no_message(snapshot, r"W10")[0])

    def test_notifications_count_as_messages(self) -> None:
        snapshot = {"messages": [], "notifications": ["Committing…"]}
        self.assertFalse(scenario.check_no_message(snapshot, "Committing")[0])

    def test_invalid_regex(self) -> None:
        ok, actual = scenario.check_no_message(sample_snapshot(), "[")
        self.assertFalse(ok)
        self.assertIn("invalid regex", actual)


class StylePredicateTest(unittest.TestCase):
    def test_dim(self) -> None:
        ansi = "\x1b[2m(--all)\x1b[0m"
        self.assertTrue(scenario.check_style(ansi, "(--all)", "dim")[0])
        self.assertFalse(scenario.check_style(ansi, "(--all)", "coloured")[0])
        self.assertFalse(scenario.check_style(ansi, "(--all)", "plain")[0])

    def test_bold(self) -> None:
        ansi = "\x1b[1m(--all)\x1b[0m"
        self.assertTrue(scenario.check_style(ansi, "(--all)", "bold")[0])

    def test_coloured(self) -> None:
        ansi = "\x1b[36m(--all)\x1b[0m"
        self.assertTrue(scenario.check_style(ansi, "(--all)", "coloured")[0])
        self.assertFalse(scenario.check_style(ansi, "(--all)", "plain")[0])

    def test_plain(self) -> None:
        self.assertTrue(scenario.check_style("(--all)", "(--all)", "plain")[0])

    def test_text_spanning_runs(self) -> None:
        ansi = "prefix (" + "\x1b[36m--all\x1b[0m" + ") suffix"
        self.assertTrue(scenario.check_style(ansi, "(--all)", "coloured")[0])
        self.assertFalse(scenario.check_style(ansi, "(--all)", "dim")[0])
        self.assertFalse(scenario.check_style(ansi, "(--all)", "plain")[0])

    def test_dim_spanning_runs(self) -> None:
        ansi = "(" + "\x1b[2m--all\x1b[0m" + ")"
        self.assertTrue(scenario.check_style(ansi, "(--all)", "dim")[0])
        self.assertFalse(scenario.check_style(ansi, "(--all)", "coloured")[0])

    def test_missing_text(self) -> None:
        ok, actual = scenario.check_style("nothing", "(--all)", "dim")
        self.assertFalse(ok)
        self.assertIn("not on screen", actual)

    def test_unknown_mode(self) -> None:
        ok, actual = scenario.check_style("x", "x", "glowing")
        self.assertFalse(ok)
        self.assertIn("unknown style mode", actual)


class CursorPredicateTest(unittest.TestCase):
    def test_cursor_on_text(self) -> None:
        screen = "line one\nUnstaged changes (2)\n"
        self.assertTrue(
            scenario.check_cursor({"cursor": {"row": 1, "col": 4}}, screen, "Unstaged")[0]
        )
        self.assertFalse(
            scenario.check_cursor({"cursor": {"row": 1, "col": 4}}, screen, "changes")[0]
        )
        self.assertTrue(
            scenario.check_cursor({"cursor": {"row": 1, "col": 9}}, screen, "changes")[0]
        )

    def test_cursor_not_on_text(self) -> None:
        snapshot = {"cursor": {"row": 1, "col": 0}}
        screen = "line one\nUnstaged changes (2)\n"
        self.assertFalse(scenario.check_cursor(snapshot, screen, "changes")[0])

    def test_text_absent(self) -> None:
        snapshot = {"cursor": {"row": 0, "col": 0}}
        ok, actual = scenario.check_cursor(snapshot, "nothing\n", "Unstaged")
        self.assertFalse(ok)
        self.assertIn("does not contain", actual)

    def test_row_outside_screen(self) -> None:
        snapshot = {"cursor": {"row": 9, "col": 0}}
        ok, actual = scenario.check_cursor(snapshot, "one\n", "one")
        self.assertFalse(ok)
        self.assertIn("outside", actual)


class LatencyPredicateTest(unittest.TestCase):
    def test_measure_key_to_idle(self) -> None:
        self.assertAlmostEqual(scenario.measure_latency("?", sample_events()), 20.0)

    def test_measure_without_idle_uses_last_event(self) -> None:
        events = [
            {"id": 1, "t_ms": 5.0, "event": "key_pressed", "detail": {"steps": ["x"]}},
            {"id": 1, "t_ms": 17.0, "event": "render_applied"},
        ]
        self.assertAlmostEqual(scenario.measure_latency("x", events), 12.0)

    def test_measure_uses_last_press_of_the_key(self) -> None:
        events = [
            {"id": 1, "t_ms": 0.0, "event": "key_pressed", "detail": {"steps": ["?"]}},
            {"id": 1, "t_ms": 100.0, "event": "idle"},
            {"id": 2, "t_ms": 200.0, "event": "key_pressed", "detail": {"steps": ["?"]}},
            {"id": 2, "t_ms": 215.0, "event": "idle"},
        ]
        self.assertAlmostEqual(scenario.measure_latency("?", events), 15.0)

    def test_measure_chord(self) -> None:
        events = [
            {"id": 1, "t_ms": 0.0, "event": "key_pressed", "detail": {"steps": ["g", "?"]}},
            {"id": 1, "t_ms": 25.0, "event": "idle"},
        ]
        self.assertAlmostEqual(scenario.measure_latency("g ?", events), 25.0)
        self.assertAlmostEqual(scenario.measure_latency("?", events), 25.0)

    def test_measure_missing_key(self) -> None:
        self.assertIsNone(scenario.measure_latency("z", sample_events()))

    def test_budget_without_scale(self) -> None:
        with patch.dict(os.environ, {"UI_STRICT_LATENCY": ""}):
            ok, actual = scenario.check_latency(sample_events(), "?", "<=", 10.0)
        self.assertTrue(ok)
        self.assertIn("20.0ms", actual)
        self.assertIn("informational", actual)

    def test_budget_fails_when_strict(self) -> None:
        with patch.dict(os.environ, {"UI_STRICT_LATENCY": "1"}):
            ok, actual = scenario.check_latency(sample_events(), "?", "<=", 10.0)
        self.assertFalse(ok)
        self.assertIn("20.0ms", actual)
        self.assertNotIn("informational", actual)

    def test_missing_trace_still_fails(self) -> None:
        with patch.dict(os.environ, {"UI_STRICT_LATENCY": "1"}):
            ok, actual = scenario.check_latency([], "?", "<=", 10.0)
        self.assertFalse(ok)
        self.assertIn("no trace", actual)

    def test_budget_with_scale(self) -> None:
        with patch.dict(os.environ, {"UI_BUDGET_SCALE": "3"}):
            ok, _ = scenario.check_latency(sample_events(), "?", "<=", 10.0)
        self.assertTrue(ok)

    def test_invalid_scale_defaults_to_one(self) -> None:
        with patch.dict(os.environ, {"UI_BUDGET_SCALE": "nope"}):
            self.assertEqual(scenario.budget_scale(), 1.0)

    def test_strict_operator(self) -> None:
        with patch.dict(os.environ, {"UI_STRICT_LATENCY": "1"}):
            ok, _ = scenario.check_latency(sample_events(), "?", "<", 20.0)
        self.assertFalse(ok)

    def test_strict_latency_flag(self) -> None:
        with patch.dict(os.environ, {"UI_STRICT_LATENCY": "1"}):
            self.assertTrue(scenario.strict_latency())
        for value in ("", "0", "true", "yes"):
            with self.subTest(value=value):
                with patch.dict(os.environ, {"UI_STRICT_LATENCY": value}):
                    self.assertFalse(scenario.strict_latency())

    def test_evaluate_latency(self) -> None:
        expectation = scenario.parse_expectation('expect latency("?") <= 100ms')
        ok, actual = scenario.evaluate(
            expectation, screen="", ansi="", snapshot={}, events=sample_events()
        )
        self.assertTrue(ok)
        self.assertIn("20.0ms", actual)


class CombinationsTest(unittest.TestCase):
    def test_default_matrix(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp("fixture: clean\n---\n", "matrix.scn")
        )
        self.assertEqual(
            scenario.combinations(parsed),
            [
                ("100x30", "standard"),
                ("100x30", "vim"),
                ("80x24", "standard"),
                ("80x24", "vim"),
            ],
        )

    def test_size_only(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                "fixture: clean\nsize: 120x40\nsize-only: true\n---\n",
                "size.scn",
            )
        )
        self.assertEqual(
            scenario.combinations(parsed),
            [("120x40", "standard"), ("120x40", "vim")],
        )

    def test_profile_only(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                "fixture: clean\nprofile: vim\nprofile-only: true\n---\n",
                "profile.scn",
            )
        )
        self.assertEqual(
            scenario.combinations(parsed),
            [("100x30", "vim"), ("80x24", "vim")],
        )

    def test_header_values_come_first(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                "fixture: clean\nsize: 90x20\nprofile: vim\n---\n",
                "order.scn",
            )
        )
        self.assertEqual(
            scenario.combinations(parsed),
            [
                ("90x20", "vim"),
                ("90x20", "standard"),
                ("80x24", "vim"),
                ("80x24", "standard"),
            ],
        )


class ClassifyTest(unittest.TestCase):
    def test_xfail_with_failure(self) -> None:
        self.assertEqual(
            scenario.classify("bug-1", [scenario.ComboResult(False)]), "XFAIL"
        )

    def test_xpass(self) -> None:
        self.assertEqual(
            scenario.classify("bug-1", [scenario.ComboResult(True)]), "FAIL"
        )

    def test_plain_pass(self) -> None:
        self.assertEqual(
            scenario.classify(None, [scenario.ComboResult(True)]), "PASS"
        )

    def test_plain_failure(self) -> None:
        self.assertEqual(
            scenario.classify(None, [scenario.ComboResult(False)]), "FAIL"
        )

    def test_startup_failure_with_xfail_is_fail(self) -> None:
        self.assertEqual(
            scenario.classify("bug-1", [scenario.ComboResult(False, startup=True)]),
            "FAIL",
        )


class MarkdownTest(unittest.TestCase):
    def test_write_markdown(self) -> None:
        directory = tempfile.mkdtemp()
        path = Path(directory) / "tour.md"
        scenario._write_markdown(
            path,
            [("tour", "standard", "100x30", "status", "line one\nline two")],
        )
        text = path.read_text(encoding="utf-8")
        self.assertIn("# UI tour", text)
        self.assertIn("## tour — standard 100x30", text)
        self.assertIn("### status", text)
        self.assertIn("line one", text)


class ShellStepTest(unittest.TestCase):
    def test_shell_step_disables_terminal_input(self) -> None:
        directory = tempfile.mkdtemp()
        run = SimpleNamespace(
            repo_path=Path(directory), home_path=Path(directory) / "home"
        )
        result = subprocess.CompletedProcess([], 0, stdout="", stderr="")
        with patch.object(scenario.subprocess, "run", return_value=result) as shell:
            scenario._run_shell(run, "true", where=3)
        self.assertIs(shell.call_args.kwargs["stdin"], subprocess.DEVNULL)
        self.assertTrue(shell.call_args.kwargs["shell"])

    def test_shell_failures_keep_statement_command_and_error(self) -> None:
        directory = tempfile.mkdtemp()
        run = SimpleNamespace(
            repo_path=Path(directory), home_path=Path(directory) / "home"
        )
        failures = (
            (
                subprocess.CompletedProcess(
                    [], 1, stdout="", stderr="register mismatch"
                ),
                "failed (exit 1): register mismatch",
            ),
            (
                subprocess.TimeoutExpired("sleep 1", 30, stderr=b"still running"),
                "timed out after 30s: still running",
            ),
            (OSError("shell unavailable"), "could not run"),
        )
        for error, detail in failures:
            with self.subTest(detail=detail):
                patcher = patch.object(
                    scenario.subprocess,
                    "run",
                    side_effect=error if isinstance(error, BaseException) else None,
                    return_value=None if isinstance(error, BaseException) else error,
                )
                with patcher:
                    with self.assertRaises(scenario.ShellStepError) as caught:
                        scenario._run_shell(run, "test-register", where=17)
                self.assertIn("statement 17", str(caught.exception))
                self.assertIn("test-register", str(caught.exception))
                self.assertIn(detail, str(caught.exception))

    def test_shell_failure_is_reported_and_later_combinations_run(self) -> None:
        path = write_temp("fixture: clean\n---\nsh: check-register\n", "shell-fail.scn")
        directory = tempfile.mkdtemp()
        run = driver.Run(
            id="shell-fail",
            fixture="clean",
            profile="standard",
            cols=100,
            rows=30,
            color="ansi",
            directory=directory,
            repo=str(Path(directory) / "repo"),
            home=str(Path(directory) / "home"),
            socket=str(Path(directory) / "nvim.sock"),
            trace=str(Path(directory) / "trace.jsonl"),
            tmux="shell-fail",
        )
        shell_results = iter(
            [
                subprocess.CompletedProcess(
                    [], 1, stdout="", stderr="register mismatch"
                ),
                subprocess.CompletedProcess([], 0, stdout="", stderr=""),
                subprocess.CompletedProcess([], 0, stdout="", stderr=""),
                subprocess.CompletedProcess([], 0, stdout="", stderr=""),
            ]
        )
        output = io.StringIO()
        with (
            patch.object(scenario.driver, "start", return_value=run) as start,
            patch.object(scenario.driver, "stop") as stop,
            patch.object(scenario, "_clear_command_line"),
            patch.object(scenario.driver, "capture", return_value="live screen"),
            patch.object(
                scenario.subprocess,
                "run",
                side_effect=lambda *a, **kw: next(shell_results),
            ) as shell,
            contextlib.redirect_stdout(output),
        ):
            code = scenario.run([str(path)])
        text = output.getvalue()
        self.assertEqual(code, 1)
        self.assertIn(
            "statement 1: `sh: check-register` failed (exit 1): register mismatch",
            text,
        )
        self.assertIn("live screen", text)
        self.assertIn("Summary: 0 PASS, 0 XFAIL, 1 FAIL", text)
        self.assertEqual(start.call_count, 4)
        self.assertEqual(stop.call_count, 4)
        self.assertEqual(shell.call_count, 4)

    def test_all_scenario_rpc_helpers_are_headless(self) -> None:
        for path in sorted((ROOT / "tests/ui/scenarios").glob("*.scn")):
            for number, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), 1
            ):
                if "--remote-expr" in line and "nvim " in line:
                    with self.subTest(path=path.name, line=number):
                        self.assertIn("nvim --headless --server", line)


class StartupReportingTest(unittest.TestCase):
    def test_four_combo_failure_reports_reason_once_and_keeps_all_artifacts(self) -> None:
        path = write_temp("fixture: clean\n---\n", "four-fail.scn")
        combos = [("100x30", "standard"), ("100x30", "vim"),
                  ("80x24", "standard"), ("80x24", "vim")]
        failure = scenario.ComboResult(
            ok=False, reason="case.scn:4: expect screen contains 'missing': absent",
            screen="trimmed failure screen", profile="", size="",
        )
        with tempfile.TemporaryDirectory() as directory:
            def fail(scenario_arg, size, profile, snapshots, verbose=False):
                return scenario.ComboResult(
                    **{**failure.__dict__, "profile": profile, "size": size}
                )
            output = io.StringIO()
            with (
                patch.object(scenario, "BUILD_UI", Path(directory)),
                patch.object(scenario, "combinations", return_value=combos),
                patch.object(scenario, "_run_combo", side_effect=fail),
                patch.object(scenario.driver, "check_tools"),
                contextlib.redirect_stdout(output),
            ):
                self.assertEqual(scenario.run([str(path)]), 1)
            text = output.getvalue()
            self.assertEqual(text.count(failure.reason), 1)
            self.assertEqual(text.count("trimmed failure screen"), 1)
            for size, profile in combos:
                self.assertIn(
                    str(Path(directory) / f"four-fail/{profile}-{size}/failure.screen"),
                    text,
                )
            self.assertEqual(
                len(list((Path(directory) / "four-fail").glob("*/failure.screen"))),
                4,
            )

    def test_distinct_combo_causes_are_each_reported_once(self) -> None:
        path = write_temp("fixture: clean\n---\n", "distinct-fail.scn")
        combos = [("100x30", "standard"), ("100x30", "vim"),
                  ("80x24", "standard"), ("80x24", "vim")]
        reasons = ["case.scn:4: first cause", "case.scn:7: second cause",
                   "case.scn:4: first cause", "case.scn:7: second cause"]
        failures = iter(reasons)
        def fail(scenario_arg, size, profile, snapshots, verbose=False):
            return scenario.ComboResult(
                ok=False, reason=next(failures), screen="one visible screen",
                profile=profile, size=size,
            )
        with tempfile.TemporaryDirectory() as directory:
            output = io.StringIO()
            with (
                patch.object(scenario, "BUILD_UI", Path(directory)),
                patch.object(scenario, "combinations", return_value=combos),
                patch.object(scenario, "_run_combo", side_effect=fail),
                patch.object(scenario.driver, "check_tools"),
                contextlib.redirect_stdout(output),
            ):
                self.assertEqual(scenario.run([str(path)]), 1)
            text = output.getvalue()
            self.assertEqual(text.count(reasons[0]), 1)
            self.assertEqual(text.count(reasons[1]), 1)
            self.assertEqual(text.count("one visible screen"), 1)
            for size, profile in combos:
                self.assertIn(
                    str(Path(directory) / f"distinct-fail/{profile}-{size}/failure.screen"),
                    text,
                )

    def test_expectation_failure_uses_scenario_source_location(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp("fixture: clean\n---\n\nexpect screen contains 'x'\n", "case.scn")
        )
        output = SimpleNamespace(repo_path=Path("/tmp"), home_path=Path("/tmp"))
        with (
            patch.object(scenario.driver, "start", return_value=output),
            patch.object(scenario, "_clear_command_line"),
            patch.object(scenario.driver, "capture", return_value="actual"),
            patch.object(scenario.driver, "snapshot", return_value={}),
            patch.object(scenario.driver, "read_trace", return_value=[]),
            patch.object(scenario, "evaluate", return_value=(False, "missing x")),
            patch.object(scenario.driver, "stop"),
        ):
            result = scenario._run_combo(parsed, "100x30", "standard", [])
        self.assertIn("case.scn:4: expect screen contains", result.reason)

    def test_startup_failure_is_labelled_separately(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                "fixture: clean\n---\nkeys: q\n", "startup-failure.scn"
            )
        )
        result = scenario.ComboResult(
            ok=False,
            reason="startup: socket: missing",
            profile="vim",
            size="100x30",
            startup=True,
        )
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            scenario._report_failure(parsed, result)
        text = output.getvalue()
        self.assertIn("STARTUP FAILED", text)
        self.assertIn("not scenario behaviour", text)
        self.assertNotIn("expect:", text)

    def test_quiet_xfail_saves_screen_without_reporting_it(self) -> None:
        path = write_temp("fixture: clean\nxfail: known\n---\n", "quiet-xfail.scn")
        result = scenario.ComboResult(
            ok=False, reason="failure detail", screen="screen row   \n   \n",
            profile="standard", size="100x30",
        )
        with tempfile.TemporaryDirectory() as directory:
            output = io.StringIO()
            with (
                patch.object(scenario, "BUILD_UI", Path(directory)),
                patch.object(scenario, "combinations", return_value=[("100x30", "standard")]),
                patch.object(scenario, "_run_combo", return_value=result),
                patch.object(scenario.driver, "check_tools"),
                contextlib.redirect_stdout(output),
            ):
                self.assertEqual(scenario.run([str(path)]), 0)
            artifact = Path(directory) / "quiet-xfail/standard-100x30/failure.screen"
            self.assertEqual(artifact.read_text(encoding="utf-8"), "screen row\n")
            text = output.getvalue()
            self.assertNotIn("screen row", text)
            self.assertNotIn("failure detail", text)
            self.assertIn("Summary: 0 PASS, 1 XFAIL, 0 FAIL", text)

    def test_scenario_failure_keeps_the_expectation_label(self) -> None:
        parsed = scenario.parse_scenario(
            write_temp(
                "fixture: clean\n---\nkeys: q\n", "behaviour-failure.scn"
            )
        )
        result = scenario.ComboResult(
            ok=False,
            reason="statement 1: screen contains 'x'",
            profile="vim",
            size="100x30",
        )
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            scenario._report_failure(parsed, result)
        text = output.getvalue()
        self.assertIn("FAILED", text)
        self.assertNotIn("STARTUP FAILED", text)
        self.assertIn("expect:", text)


if __name__ == "__main__":
    unittest.main()
