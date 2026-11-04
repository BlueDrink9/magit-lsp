"""Focused tests for the report-only hygiene runner."""

import os
import subprocess
import sys
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
        git = Path("/usr/bin/git")
        if not git.exists():
            git = Path("/bin/git")
        git_link = Path(tools) / "git"
        if not git_link.exists():
            git_link.symlink_to(git)
        env = dict(os.environ, PATH=str(tools), HYGIENE_SOFT_LIMIT="20")
        return subprocess.run([sys.executable, str(SCRIPT)], cwd=self.root, env=env, text=True, capture_output=True)

    def stub(self, directory, name, code, output):
        path = directory / name
        path.write_text(f"#!/bin/sh\nprintf '%s\\n' {output!r}\nexit {code}\n")
        path.chmod(0o755)

    def test_known_findings_and_unknown_nonzero_are_distinct(self):
        with tempfile.TemporaryDirectory() as tools:
            bindir = Path(tools)
            for tool in ("cargo", "knip", "luacheck"):
                self.stub(bindir, tool, 1 if tool == "cargo" else 0, "tool output")
            result = self.execute(tools)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cargo machete: FINDINGS (exit 1)", result.stdout)
        self.assertIn("tool output", result.stdout)

    def test_arbitrary_nonzero_is_not_called_findings(self):
        (self.root / "clients/vscode").mkdir(parents=True)
        with tempfile.TemporaryDirectory() as tools:
            bindir = Path(tools)
            self.stub(bindir, "cargo", 7, "tool output")
            self.stub(bindir, "knip", 1, "config error")
            self.stub(bindir, "luacheck", 0, "")
            result = self.execute(tools)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cargo machete: NONZERO (exit 7)", result.stdout)
        self.assertIn("knip: NONZERO (exit 1)", result.stdout)
        self.assertNotIn("FINDINGS", result.stdout)

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

    def test_generated_attributes_exclude_size_and_markers(self):
        generated = self.root / "generated file.txt"
        generated.write_text("keep-for: generated\n" + "z" * 200)
        true_generated = self.root / "true-generated.txt"
        true_generated.write_text("keep-for: true generated\n" + "z" * 200)
        (self.root / ".gitattributes").write_text(
            '"generated file.txt" linguist-generated\n'
            "true-generated.txt linguist-generated=true\n"
        )
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        attributes = subprocess.run(
            ["git", "check-attr", "-z", "linguist-generated", "--", "generated file.txt", "true-generated.txt"],
            cwd=self.root, check=True, capture_output=True,
        ).stdout.split(b"\0")
        self.assertEqual(attributes[2], b"set")
        self.assertEqual(attributes[5], b"true")
        with tempfile.TemporaryDirectory() as tools:
            for tool in ("cargo", "knip", "luacheck"):
                self.stub(Path(tools), tool, 0, "")
            result = self.execute(tools)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("generated file.txt", result.stdout)
        self.assertNotIn("true-generated.txt", result.stdout)
        self.assertIn("big.txt", result.stdout)

    def test_invocation_failure_is_reported_and_knip_uses_client_cwd(self):
        (self.root / "clients/vscode").mkdir(parents=True)
        with tempfile.TemporaryDirectory() as tools:
            bindir = Path(tools)
            self.stub(bindir, "cargo", 0, "")
            self.stub(bindir, "knip", 0, "")
            self.stub(bindir, "luacheck", 0, "")
            (bindir / "knip").write_text("#!/no/such/interpreter\n")
            (bindir / "knip").chmod(0o755)
            result = self.execute(tools)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("knip: INVOCATION FAILURE:", result.stdout)

    def test_missing_cargo_machete_subcommand_is_nonzero(self):
        with tempfile.TemporaryDirectory() as tools:
            bindir = Path(tools)
            self.stub(bindir, "cargo", 101, "no such command: machete")
            self.stub(bindir, "knip", 0, "")
            self.stub(bindir, "luacheck", 0, "")
            result = self.execute(tools)
        self.assertIn("cargo machete: NONZERO (exit 101)", result.stdout)
        self.assertIn("no such command: machete", result.stdout)


if __name__ == "__main__":
    unittest.main()
