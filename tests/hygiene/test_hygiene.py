"""Focused tests for the report-only hygiene runner."""

import contextlib
import importlib.util
import io
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[2] / "scripts/hygiene.py"
REAL_GIT = shutil.which("git")


class HygieneTests(unittest.TestCase):
    """Exercise runner reporting against real temporary Git repositories."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        subprocess.run([REAL_GIT, "init", "-q"], cwd=self.root, check=True)
        (self.root / "big.txt").write_text("keep-for: reason\nallow(dead_code)\n" + "x\n" * 30)
        (self.root / "small.txt").write_text("small\n")
        subprocess.run([REAL_GIT, "add", "."], cwd=self.root, check=True)
        spec = importlib.util.spec_from_file_location("hygiene_under_test", SCRIPT)
        self.runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.runner)

    def tearDown(self):
        self.temp.cleanup()

    def execute(self, outcomes=None, missing=(), failures=()):
        outcomes = outcomes or {}
        calls = []
        original = subprocess.run

        def run(command, *args, **kwargs):
            if command[0] == "git":
                return original([REAL_GIT, *command[1:]], *args, **kwargs)
            calls.append((command, Path(kwargs["cwd"])))
            name = Path(command[0]).name
            if name in failures:
                raise OSError("cannot execute")
            if name in missing:
                raise AssertionError("missing executable should not launch")
            code, output = outcomes.get(name, (0, ""))
            return subprocess.CompletedProcess(command, code, output, "")

        stdout = io.StringIO()
        with patch.object(self.runner.subprocess, "run", side_effect=run), \
             patch.object(self.runner.shutil, "which", side_effect=lambda n: None if n in missing else f"/fake/{n}"), \
             contextlib.redirect_stdout(stdout):
            self.runner.report(self.root)
        return stdout.getvalue(), calls

    def test_known_findings_and_unknown_nonzero_are_distinct(self):
        output, calls = self.execute({"cargo": (1, "tool output")})
        self.assertIn("cargo machete: FINDINGS (exit 1)", output)
        self.assertIn("tool output", output)
        self.assertEqual(calls[0], (["cargo", "machete"], self.root))
        self.assertEqual(calls[1], (["knip"], self.root / "clients/vscode"))
        self.assertEqual(calls[2], (["luacheck", "--config", str(self.root / ".luacheckrc")], self.root))

    def test_nonzero_is_reported(self):
        output, _ = self.execute({"cargo": (7, "tool output"), "knip": (1, "config error")})
        self.assertIn("cargo machete: NONZERO (exit 7)", output)
        self.assertIn("knip: NONZERO (exit 1)", output)

    def test_missing_checker_is_distinct(self):
        output, calls = self.execute(missing={"knip"})
        self.assertIn("knip: MISSING executable knip", output)
        self.assertNotIn("knip: CLEAN", output)
        self.assertEqual([c[0][0] for c in calls], ["cargo", "luacheck"])

    def test_sizes_and_markers_are_deterministic(self):
        with patch.dict(os.environ, {"HYGIENE_SOFT_LIMIT": "20"}):
            spec = importlib.util.spec_from_file_location("hygiene_limited", SCRIPT)
            limited = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(limited)
        first, _ = self.execute()
        second, _ = self.execute()
        self.assertEqual(first, second)
        self.assertIn("Largest tracked text files (top 20; soft limit 100000 bytes):", first)
        self.assertIn("big.txt:1:keep-for: reason", first)
        self.assertIn("big.txt:2:allow(dead_code)", first)

        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            limited.report(self.root)
        self.assertIn("soft limit 20 bytes", output.getvalue())
        self.assertIn("[over soft limit]", output.getvalue())

    def test_generated_and_untracked_lua_excluded_but_tracked_lua_passed(self):
        generated = self.root / "generated space.lua"
        generated.write_text("-- keep-for: generated-marker\n" + "x\n" * 30)
        generated_true = self.root / "generated-true.txt"
        generated_true.write_text("allow(dead_code) generated true\n" + "x\n" * 30)
        tracked = self.root / "tracked.lua"
        tracked.write_text("-- tracked\n")
        (self.root / ".gitattributes").write_text('"generated space.lua" linguist-generated=set\ngenerated-true.txt linguist-generated=true\n')
        subprocess.run([REAL_GIT, "add", "."], cwd=self.root, check=True)
        (self.root / "untracked.lua").write_text("-- untracked\n")
        output, calls = self.execute()
        self.assertNotIn("generated-marker", output)
        self.assertNotIn("generated-true", output)
        self.assertIn("small.txt", output)
        lua_call = next(c for c in calls if c[0][0] == "luacheck")
        self.assertEqual(lua_call[0], ["luacheck", "--config", str(self.root / ".luacheckrc"), "tracked.lua"])
        self.assertNotIn("generated.lua", lua_call[0])
        self.assertNotIn("untracked.lua", lua_call[0])

    def test_invocation_failure_is_reported(self):
        output, _ = self.execute(failures={"knip"})
        self.assertIn("knip: INVOCATION FAILURE: cannot execute", output)

    def test_missing_cargo_machete_subcommand_is_nonzero(self):
        output, _ = self.execute({"cargo": (101, "no such command: machete")})
        self.assertIn("cargo machete: NONZERO (exit 101)", output)


if __name__ == "__main__":
    unittest.main()
