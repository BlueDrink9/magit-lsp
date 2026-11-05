"""Integration tests for staged-safe formatter checks."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
RUNNER = ROOT / "scripts/format.py"
STYLUA = ROOT / "clients/vscode/node_modules/.bin/stylua"
PRETTIER = ROOT / "clients/vscode/node_modules/.bin/prettier"


class FormatIntegration(unittest.TestCase):
    """Exercise formatter modes against isolated real Git repositories."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.email", "format@example.test")
        self.git("config", "user.name", "Format Test")
        self.git("commit", "--allow-empty", "-qm", "base")

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.repo, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout

    def write(self, path, text):
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def check(self, *args, env=None):
        return subprocess.run(["python3", str(RUNNER), *args], cwd=self.repo, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    def tools(self):
        if not STYLUA.exists() or not PRETTIER.exists():
            self.skipTest("locked formatters are not installed in checkout")

    def test_staged_good_content_ignores_bad_working_copy_and_preserves_index(self):
        self.tools()
        self.write("lua/a.lua", 'local x = "ok"\n')
        self.git("add", "lua/a.lua")
        staged = self.git("show", ":lua/a.lua")
        self.write("lua/a.lua", 'local x="dirty"\n')
        status = self.git("status", "--porcelain")
        result = self.check("--staged")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(self.git("show", ":lua/a.lua"), staged)
        self.assertEqual(self.git("status", "--porcelain"), status)

    def test_bad_staged_content_fails_even_if_working_copy_is_clean(self):
        self.tools()
        self.write("clients/a.js", "const x={a:1}\n")
        self.git("add", "clients/a.js")
        self.write("clients/a.js", "const x = { a: 1 };\n")
        status = self.git("status", "--porcelain")
        result = self.check("--staged")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("Prettier 3.9.10", result.stdout)
        self.assertEqual(self.git("show", ":clients/a.js"), b"const x={a:1}\n")
        self.assertEqual(self.git("status", "--porcelain"), status)

    def test_base_mode_changed_scope_and_staged_parity(self):
        self.tools()
        self.write("clients/a.ts", "const x: number = 1;\n")
        self.git("add", "clients/a.ts")
        self.git("commit", "-qm", "formatted source")
        base = self.git("rev-parse", "HEAD^").decode().strip()
        staged = self.check("--staged")
        ranged = self.check("--base", base)
        self.assertEqual(staged.returncode, ranged.returncode, staged.stdout + ranged.stdout)

    def test_bad_staged_and_committed_range_have_same_result_with_rename_and_spaces(self):
        self.tools()
        self.write("clients/old name.ts", "const x:number=1\n")
        self.git("add", "clients/old name.ts")
        staged = self.check("--staged")
        self.assertNotEqual(staged.returncode, 0, staged.stdout)
        self.git("commit", "-qm", "bad source")
        base = self.git("rev-parse", "HEAD^").decode().strip()
        self.git("mv", "clients/old name.ts", "clients/new name.ts")
        ranged = self.check("--base", base)
        self.assertEqual(staged.returncode, ranged.returncode, staged.stdout + ranged.stdout)

    def test_syntax_error_fails_with_formatter_output(self):
        self.tools()
        self.write("clients/broken.js", "const = ;\n")
        self.git("add", "clients/broken.js")
        result = self.check("--staged")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("Prettier 3.9.10", result.stdout)

    def test_changed_rust_invokes_workspace_command(self):
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/a"]\nresolver = "2"\n')
        self.write("crates/a/Cargo.toml", '[package]\nname="a"\nversion="0.1.0"\nedition="2021"\n')
        self.write("crates/a/src/lib.rs", "pub fn f() {}\n")
        self.git("add", ".")
        bindir = self.repo / "fake-bin"
        bindir.mkdir()
        marker = self.repo / "cargo-args"
        cargo = bindir / "cargo"
        cargo.write_text(f'#!/bin/sh\nprintf "%s\\n" "$*" > "{marker}"\n')
        cargo.chmod(0o755)
        env = os.environ.copy()
        env["PATH"] = str(bindir) + os.pathsep + env["PATH"]
        result = self.check("--staged", env=env)
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(marker.read_text().strip(), "fmt --all -- --check")

    def test_initial_commit_and_deletion_do_not_crash(self):
        self.write("lua/delete.lua", 'local x = "x"\n')
        self.git("add", "lua/delete.lua")
        self.git("commit", "-qm", "add")
        self.git("rm", "lua/delete.lua")
        self.assertEqual(self.check("--staged").returncode, 0)


if __name__ == "__main__":
    unittest.main()
