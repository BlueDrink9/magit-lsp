"""Focused tests for the report-only hygiene runner."""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[2] / "scripts/hygiene.py"


class HygieneTests(unittest.TestCase):
    """Exercise runner reporting against real temporary Git repositories."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        (self.root / "big.txt").write_text("keep-for: reason\nallow(dead_code)\n" + "x\n" * 30)
        (self.root / "small.txt").write_text("small\n")
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)

    def tearDown(self):
        self.temp.cleanup()

    def execute(self, tools):
        env = dict(os.environ, PATH=f"{tools}{os.pathsep}{os.environ['PATH']}", HYGIENE_SOFT_LIMIT="20")
        return subprocess.run(["python3", str(SCRIPT)], cwd=self.root, env=env, text=True, capture_output=True)

    def stub(self, directory, name, code, output):
        path = directory / name
        path.write_text(f"#!/bin/sh\nprintf '%s\\n' {output!r}\nexit {code}\n")
        path.chmod(0o755)

    def test_nonzero_checker_is_reported_but_runner_succeeds(self):
        with tempfile.TemporaryDirectory() as tools:
            bindir = Path(tools)
            for tool in ("cargo", "knip", "luacheck"):
                self.stub(bindir, tool, 7 if tool == "cargo" else 0, "tool output")
            result = self.execute(tools)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cargo machete: FINDINGS (exit 7)", result.stdout)
        self.assertIn("tool output", result.stdout)

    def test_missing_checker_is_distinct(self):
        with tempfile.TemporaryDirectory() as tools:
            self.stub(Path(tools), "cargo", 0, "")
            self.stub(Path(tools), "luacheck", 0, "")
            result = self.execute(tools)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("knip: MISSING executable knip", result.stdout)
        self.assertNotIn("knip: CLEAN", result.stdout)

    def test_sizes_and_markers_are_deterministic(self):
        with tempfile.TemporaryDirectory() as tools:
            for tool in ("cargo", "knip", "luacheck"):
                self.stub(Path(tools), tool, 0, "")
            first = self.execute(tools)
            second = self.execute(tools)
        self.assertEqual(first.stdout, second.stdout)
        self.assertIn("Largest tracked text files (top 20; soft limit 20 bytes):", first.stdout)
        self.assertIn("big.txt", first.stdout)
        self.assertIn("[over soft limit]", first.stdout)
        self.assertIn("big.txt:1:keep-for: reason", first.stdout)
        self.assertIn("big.txt:2:allow(dead_code)", first.stdout)


if __name__ == "__main__":
    unittest.main()
