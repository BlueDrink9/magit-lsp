"""Unit tests for the deterministic fixtures (U0.3)."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from ui_lib import fixtures  # noqa: E402


def status(repo: Path, env: dict[str, str]) -> str:
    return fixtures.git(repo, env, "status", "--porcelain=v2", "--branch").stdout


def head(repo: Path, env: dict[str, str]) -> str:
    result = fixtures.git(repo, env, "rev-parse", "--verify", "HEAD", check=False)
    return result.stdout.strip() if result.returncode == 0 else ""


def entries(text: str) -> list[str]:
    return [line for line in text.splitlines() if not line.startswith("#")]


class FixtureTest(unittest.TestCase):
    def build(self, name: str):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        repo = fixtures.build(name, root / "repo", root / "home")
        env = fixtures.determinism_env(root / "home")
        return repo, env

    def test_fixture_list(self) -> None:
        self.assertEqual(
            set(fixtures.FIXTURES),
            {
                "clean",
                "modified-two-files",
                "staged-one-file",
                "staged-and-unstaged",
                "staged-failing-hook",
                "untracked",
                "unborn",
                "in-progress-rebase",
                "in-progress-cherry-pick",
            },
        )

    def test_unknown_fixture_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                fixtures.build("nope", Path(tmp) / "repo", Path(tmp) / "home")

    def test_non_empty_destination_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "repo"
            dest.mkdir()
            (dest / "leftover").write_text("x", encoding="utf-8")
            with self.assertRaises(ValueError):
                fixtures.build("clean", dest, Path(tmp) / "home")

    def test_fixtures_are_deterministic_across_runs(self) -> None:
        for name in fixtures.FIXTURES:
            with self.subTest(fixture=name):
                first_repo, first_env = self.build(name)
                first = (
                    head(first_repo, first_env),
                    status(first_repo, first_env),
                )
                second_repo, second_env = self.build(name)
                second = (
                    head(second_repo, second_env),
                    status(second_repo, second_env),
                )
                self.assertEqual(first, second)

    def test_clean_has_no_changes(self) -> None:
        repo, env = self.build("clean")
        self.assertEqual(entries(status(repo, env)), [])

    def test_modified_two_files(self) -> None:
        repo, env = self.build("modified-two-files")
        changes = entries(status(repo, env))
        self.assertEqual(len(changes), 2)
        self.assertTrue(all(" .M " in line for line in changes))
        self.assertTrue(any(line.endswith("f.txt") for line in changes))
        self.assertTrue(any(line.endswith("g.txt") for line in changes))

    def test_staged_one_file(self) -> None:
        repo, env = self.build("staged-one-file")
        changes = entries(status(repo, env))
        self.assertEqual(len(changes), 1)
        self.assertIn(" M. ", changes[0])
        self.assertTrue(changes[0].endswith("f.txt"))

    def test_staged_and_unstaged(self) -> None:
        repo, env = self.build("staged-and-unstaged")
        changes = entries(status(repo, env))
        self.assertEqual(len(changes), 2)
        self.assertTrue(any(" M. " in line and line.endswith("f.txt") for line in changes))
        self.assertTrue(any(" .M " in line and line.endswith("g.txt") for line in changes))

    def test_untracked(self) -> None:
        repo, env = self.build("untracked")
        changes = entries(status(repo, env))
        self.assertEqual(changes, ["? new.txt"])

    def test_unborn(self) -> None:
        repo, env = self.build("unborn")
        self.assertEqual(head(repo, env), "")
        changes = entries(status(repo, env))
        self.assertEqual(len(changes), 1)
        self.assertTrue(changes[0].startswith("1 A. "))
        self.assertTrue(changes[0].endswith("f.txt"))

    def test_in_progress_rebase(self) -> None:
        repo, env = self.build("in-progress-rebase")
        self.assertTrue((repo / ".git" / "rebase-merge").is_dir())
        self.assertIn("u UU ", status(repo, env))

    def test_in_progress_cherry_pick(self) -> None:
        repo, env = self.build("in-progress-cherry-pick")
        self.assertTrue((repo / ".git" / "CHERRY_PICK_HEAD").is_file())
        self.assertIn("u UU ", status(repo, env))


if __name__ == "__main__":
    unittest.main()
