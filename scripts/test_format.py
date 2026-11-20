#!/usr/bin/env python3
"""Test the formatter dispatcher's all-tracked sweep and hook safety.

Each test builds a throwaway Git repository with tracked, excluded and untracked
maintained sources, puts fake `rustfmt`, `stylua` and `prettier` executables on
PATH that log their arguments, and runs the production `scripts/format.py`.
Run via `make format-test`.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FORMAT_PY = REPO / "scripts/format.py"
HOOK = REPO / ".githooks/pre-commit"

FAKE_FORMATTER = '''#!/usr/bin/env python3
"""Log invocations and pretend to format or check source files."""
import os
import sys
from pathlib import Path

EXTENSIONS = {".lua", ".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts", ".rs"}
log = Path(os.environ["FAKE_FORMAT_LOG"])
with log.open("a", encoding="utf-8") as handle:
    handle.write(Path(sys.argv[0]).name + " " + " ".join(sys.argv[1:]) + "\\n")
check = "--check" in sys.argv[1:]
status = 0
for argument in sys.argv[1:]:
    if argument.startswith("-"):
        continue
    candidate = Path(argument)
    if candidate.suffix not in EXTENSIONS:
        continue
    if check:
        if not candidate.is_file() or candidate.read_text() != "ok\\n":
            status = 1
    elif candidate.is_file():
        candidate.write_text("ok\\n")
sys.exit(status)
'''

SEED = {
    "Cargo.toml": '[package]\nname = "fixture"\nversion = "0.0.0"\nedition = "2021"\n',
    "src/main.rs": "ok\n",
    "src/lib.rs": "ok\n",
    "lua/magit-lsp/ok.lua": "ok\n",
    "clients/vscode/src/ok.ts": "ok\n",
    "vendor/vendor.lua": "ok\n",
    "generated/generated.rs": "ok\n",
    "tests/fixtures/fixture.lua": "ok\n",
    "node_modules/package/index.js": "ok\n",
    "docs/not_owned.lua": "ok\n",
    "notes.txt": "ok\n",
}


class FormatSweepTest(unittest.TestCase):
    """Exercise all-tracked selection, routing and index-only hook behavior."""

    def setUp(self) -> None:
        self.repo = Path(tempfile.mkdtemp(prefix="magit-format-repo-"))
        self.addCleanup(shutil.rmtree, self.repo, ignore_errors=True)
        self.tools = Path(tempfile.mkdtemp(prefix="magit-format-tools-"))
        self.addCleanup(shutil.rmtree, self.tools, ignore_errors=True)
        self.log = self.tools / "calls.log"
        tool = self.tools / "fake_formatter.py"
        tool.write_text(FAKE_FORMATTER)
        tool.chmod(0o755)
        for name in ("rustfmt", "stylua", "prettier"):
            (self.tools / name).symlink_to(tool)
        self.env = {
            key: value
            for key, value in os.environ.items()
            if key not in {"GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"}
        }
        self.env.update(
            PATH=str(self.tools) + os.pathsep + self.env.get("PATH", ""),
            FAKE_FORMAT_LOG=str(self.log),
            PYTHONDONTWRITEBYTECODE="1",
        )
        self.git("init", "-q")
        self.git("config", "user.email", "format@example.com")
        self.git("config", "user.name", "Format Test")
        for path, text in SEED.items():
            self.write(path, text)
        self.git("add", "-A")
        self.git("commit", "-qm", "seed")
        self.write("lua/magit-lsp/untracked.lua", "ok\n")
        prettier = self.repo / "clients/vscode/node_modules/.bin/prettier"
        prettier.parent.mkdir(parents=True, exist_ok=True)
        prettier.symlink_to(self.tools / "prettier")

    def git(self, *args: str) -> str:
        """Run Git in the fixture repository and return decoded stdout."""
        result = subprocess.run(
            ["git", *args], cwd=self.repo, env=self.env, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
        )
        return result.stdout

    def write(self, path: str, text: str) -> None:
        """Write one fixture file, creating parents."""
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def read(self, path: str) -> str:
        """Read one fixture file."""
        return (self.repo / path).read_text()

    def run_format(self, *args: str) -> subprocess.CompletedProcess[str]:
        """Run the production dispatcher inside the fixture repository."""
        return subprocess.run(
            [sys.executable, str(FORMAT_PY), *args], cwd=self.repo, env=self.env,
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )

    def run_hook(self, **extra: str) -> subprocess.CompletedProcess[str]:
        """Run the production pre-commit hook inside the fixture repository."""
        env = {**self.env, **extra}
        return subprocess.run(
            ["sh", str(self.repo / ".githooks/pre-commit")], cwd=self.repo, env=env,
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )

    def install_hook(self) -> None:
        """Copy the production dispatcher and hook into the fixture."""
        (self.repo / "scripts").mkdir(exist_ok=True)
        shutil.copy2(FORMAT_PY, self.repo / "scripts/format.py")
        (self.repo / ".githooks").mkdir(exist_ok=True)
        hook = self.repo / ".githooks/pre-commit"
        shutil.copy2(HOOK, hook)
        hook.chmod(0o755)

    def calls(self) -> list[str]:
        """Return the formatter invocation log lines."""
        return [line for line in self.log.read_text().splitlines() if line]

    def test_all_tracked_selection_and_exclusions(self) -> None:
        """All-tracked mode selects only owned tracked Lua files."""
        result = self.run_format("--all-tracked", "--formatter", "stylua")
        self.assertEqual(0, result.returncode, result.stderr)
        calls = self.calls()
        self.assertTrue(calls)
        self.assertTrue(all(call.startswith("stylua ") for call in calls), calls)
        joined = "\n".join(calls)
        self.assertIn("lua/magit-lsp/ok.lua", joined)
        for excluded in (
            "untracked.lua", "vendor/", "generated/", "fixtures/",
            "node_modules/", "docs/not_owned.lua", "notes.txt",
        ):
            self.assertNotIn(excluded, joined)

    def test_selected_formatter_routing(self) -> None:
        """Selecting one formatter never invokes the other tools."""
        result = self.run_format("--all-tracked", "--formatter", "rustfmt")
        self.assertEqual(0, result.returncode, result.stderr)
        calls = self.calls()
        self.assertTrue(calls)
        self.assertTrue(all(call.startswith("rustfmt ") for call in calls), calls)
        joined = "\n".join(calls)
        self.assertIn("src/main.rs", joined)
        self.assertIn("src/lib.rs", joined)
        self.assertNotIn(".lua", joined)
        self.assertNotIn(".ts", joined)

    def test_full_check_fails_on_unformatted_tracked_file(self) -> None:
        """The full check is fail-capable for already tracked files."""
        self.write("src/main.rs", "bad\n")
        self.git("add", "src/main.rs")
        self.git("commit", "-qm", "bad rust")
        result = self.run_format("--all-tracked", "--formatter", "rustfmt")
        self.assertEqual(1, result.returncode)
        self.write("src/main.rs", "ok\n")
        self.git("add", "src/main.rs")
        self.git("commit", "-qm", "format rust")
        result = self.run_format("--all-tracked", "--formatter", "rustfmt")
        self.assertEqual(0, result.returncode, result.stderr)

    def test_write_refuses_unstaged_changes(self) -> None:
        """All-tracked writes never overwrite a dirty working copy."""
        self.write("src/main.rs", "bad\n")
        self.git("add", "src/main.rs")
        self.git("commit", "-qm", "bad rust")
        result = self.run_format("--all-tracked", "--formatter", "rustfmt", "--write")
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.read("src/main.rs"))
        self.git("add", "-A")
        self.git("commit", "-qm", "formatted rust")
        self.write("src/main.rs", "dirty\n")
        result = self.run_format("--all-tracked", "--formatter", "rustfmt", "--write")
        self.assertEqual(1, result.returncode)
        self.assertIn("refusing to overwrite unstaged changes", result.stderr)
        self.assertEqual("dirty\n", self.read("src/main.rs"))

    def test_ordinary_hook_formats_index_only(self) -> None:
        """The default hook fixes staged blobs and preserves divergent working files."""
        self.install_hook()
        path = "lua/magit-lsp/ok.lua"
        self.write(path, "bad\n")
        self.git("add", path)
        self.write(path, "worktree\n")
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.git("show", f":{path}"))
        self.assertEqual("worktree\n", self.read(path))

    def test_hook_syncs_matching_working_copy(self) -> None:
        """A staged-only change is formatted in the index and its working copy."""
        self.install_hook()
        path = "lua/magit-lsp/ok.lua"
        self.write(path, "bad\n")
        self.git("add", path)
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.git("show", f":{path}"))
        self.assertEqual("ok\n", self.read(path))
        self.assertEqual("", self.git("status", "--porcelain", "--untracked-files=no", "--", path))

    def test_hook_preserves_partially_staged_working_edits(self) -> None:
        """Unstaged additions to a staged file survive while the index formats."""
        self.install_hook()
        path = "lua/magit-lsp/ok.lua"
        self.write(path, "bad\n")
        self.git("add", path)
        self.write(path, "bad\nunstaged\n")
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.git("show", f":{path}"))
        self.assertEqual("bad\nunstaged\n", self.read(path))

    def test_hook_sweep_requires_all_tracked_formatted(self) -> None:
        """The opt-in sweep guard fails a partial format-only commit."""
        self.install_hook()
        self.write("src/main.rs", "bad\n")
        self.write("src/lib.rs", "bad\n")
        self.git("add", "-A")
        self.git("commit", "-qm", "bad rust")
        self.write("src/main.rs", "staged\n")
        self.git("add", "src/main.rs")
        result = self.run_hook(MAGIT_FORMAT_SWEEP="rustfmt")
        self.assertNotEqual(0, result.returncode)
        self.assertEqual("bad\n", self.git("show", ":src/lib.rs"))
        self.write("src/lib.rs", "ok\n")
        self.git("add", "src/lib.rs")
        self.git("commit", "-qm", "formatted rust")
        result = self.run_hook(MAGIT_FORMAT_SWEEP="rustfmt")
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.git("show", ":src/lib.rs"))


if __name__ == "__main__":
    unittest.main()
