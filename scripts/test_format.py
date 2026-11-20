#!/usr/bin/env python3
"""Test the formatter dispatcher's all-tracked sweep and hook safety.

Each test builds a throwaway Git repository with tracked, excluded and untracked
maintained sources, puts fake `rustfmt`, `stylua` and `prettier` executables on
PATH that log their arguments, and runs the production `scripts/format.py`.
`CIWorkflowEnvironmentTest` also checks that every Linux CI job runs its tools
inside the locked flake dev shell, using source-level negative fixtures.
Run via `make format-test`.
"""
from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FORMAT_PY = REPO / "scripts/format.py"
HOOK = REPO / ".githooks/pre-commit"
CI_WORKFLOW = REPO / ".github/workflows/ci.yml"
NIX_DEVELOP = "nix develop --no-update-lock-file -c"

# Every Linux job must run its tool commands inside the flake's locked dev
# shell, and it must not provision any of these tools itself.
LINUX_JOB_COMMANDS = {
    "format-check": (
        f"{NIX_DEVELOP} make vscode-deps",
        f'{NIX_DEVELOP} python3 scripts/format.py --base "$base"',
    ),
    "release-contract": (f"{NIX_DEVELOP} make release-contract-check",),
    "test": (
        f"{NIX_DEVELOP} make vscode-test",
        f"{NIX_DEVELOP} make test-rust test-lua",
    ),
    "coverage": (f"{NIX_DEVELOP} make coverage-rust",),
    "documentation": (f"{NIX_DEVELOP} make docs-artifacts-check docs-check",),
    "ui-test": (
        f"{NIX_DEVELOP} make build",
        f"{NIX_DEVELOP} env CI=1 make ui-unit ui-test",
    ),
}

FORBIDDEN_PROVISIONING = (
    "cargo install",
    "apt-get install",
    "dtolnay/rust-toolchain",
    "actions/setup-node",
    "actions/setup-python",
    "pnpm/action-setup",
    "rhysd/action-setup-vim",
)

# `scripts/generate_schema.rs` owns these bytes: `write_all` writes them and
# `stale_files` compares fresh generator output byte-for-byte. The formatter
# must skip them by exact path while the handwritten files beside them stay
# eligible, so these fixtures mirror the real tree layout.
GENERATOR_OWNED = (
    "lua/magit-lsp/types.lua",
    "lua/magit-lsp/shared.lua",
    "clients/vscode/types/protocol.d.ts",
    "clients/vscode/types/shared-lists.js",
)
GENERATOR_NEIGHBORS = (
    "lua/magit-lsp/handwritten.lua",
    "clients/vscode/types/handwritten.ts",
    "clients/vscode/types/handwritten.js",
)


def workflow_jobs(workflow: str) -> dict[str, str]:
    """Return each job body from a workflow's top-level `jobs:` mapping."""
    lines = workflow.splitlines()
    try:
        start = lines.index("jobs:")
    except ValueError:
        return {}
    jobs: dict[str, list[str]] = {}
    current = None
    for line in lines[start + 1:]:
        match = re.match(r"^  ([A-Za-z0-9_-]+):$", line)
        if match:
            current = match.group(1)
            jobs[current] = []
        elif current is not None:
            jobs[current].append(line)
    return {name: "\n".join(body) for name, body in jobs.items()}


def workflow_environment_violations(workflow: str) -> list[str]:
    """Report Linux CI jobs that leave the locked flake dev shell."""
    jobs = workflow_jobs(workflow)
    violations = []
    for name, commands in LINUX_JOB_COMMANDS.items():
        body = jobs.get(name)
        if body is None:
            violations.append(f"{name}: job is missing")
            continue
        for command in commands:
            if command not in body:
                violations.append(f"{name}: missing locked command: {command}")
        for forbidden in FORBIDDEN_PROVISIONING:
            if forbidden in body:
                violations.append(
                    f"{name}: provisions tools outside the dev shell with '{forbidden}'"
                )
    native = jobs.get("native-test", "")
    if "nix develop" in native:
        violations.append("native-test: native lanes must not claim Nix coverage")
    return violations


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
if "--config-path" in sys.argv:
    config = Path(sys.argv[sys.argv.index("--config-path") + 1])
    if not config.is_file():
        print(f"missing config: {config}", file=sys.stderr)
        status = 1
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
    **{path: "ok\n" for path in GENERATOR_OWNED + GENERATOR_NEIGHBORS},
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

    def test_all_tracked_skips_generator_owned_client_references(self) -> None:
        """Generator bytes are excluded by exact path while neighbors stay eligible."""
        result = self.run_format("--all-tracked", "--formatter", "stylua")
        self.assertEqual(0, result.returncode, result.stderr)
        joined = "\n".join(self.calls())
        for neighbor in ("lua/magit-lsp/ok.lua", "lua/magit-lsp/handwritten.lua"):
            self.assertIn(neighbor, joined)
        for generated in ("lua/magit-lsp/types.lua", "lua/magit-lsp/shared.lua"):
            self.assertNotIn(generated, joined)
        self.log.unlink()
        result = self.run_format("--all-tracked", "--formatter", "prettier")
        self.assertEqual(0, result.returncode, result.stderr)
        joined = "\n".join(self.calls())
        for neighbor in (
            "clients/vscode/src/ok.ts",
            "clients/vscode/types/handwritten.ts",
            "clients/vscode/types/handwritten.js",
        ):
            self.assertIn(neighbor, joined)
        for generated in (
            "clients/vscode/types/protocol.d.ts",
            "clients/vscode/types/shared-lists.js",
        ):
            self.assertNotIn(generated, joined)

    def test_ordinary_hook_preserves_generator_owned_client_references(self) -> None:
        """The hook leaves generated bytes alone and still fixes eligible neighbors."""
        self.install_hook()
        generated = "lua/magit-lsp/types.lua"
        handwritten = "lua/magit-lsp/handwritten.lua"
        for path in (generated, handwritten):
            self.write(path, "bad\n")
            self.git("add", path)
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("bad\n", self.git("show", f":{generated}"))
        self.assertEqual("bad\n", self.read(generated))
        self.assertEqual("ok\n", self.git("show", f":{handwritten}"))
        self.assertEqual("bad\n", self.read(handwritten))
        joined = "\n".join(self.calls())
        self.assertIn(handwritten, joined)
        for path in GENERATOR_OWNED:
            self.assertNotIn(path, joined)

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
        """The default hook fixes staged blobs and leaves working files alone."""
        self.install_hook()
        path = "lua/magit-lsp/ok.lua"
        self.write(path, "bad\n")
        self.git("add", path)
        self.write(path, "worktree\n")
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.git("show", f":{path}"))
        self.assertEqual("worktree\n", self.read(path))

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

    def stylua_arguments(self) -> list[list[str]]:
        """Return parsed argument lists for every StyLua invocation."""
        return [call.split(" ")[1:] for call in self.calls() if call.startswith("stylua ")]

    def stylua_config_paths(self) -> list[str]:
        """Return every --config-path value passed to the fake StyLua."""
        values = []
        for arguments in self.stylua_arguments():
            if "--config-path" in arguments:
                values.append(arguments[arguments.index("--config-path") + 1])
        return values

    def test_stylua_uses_workspace_snapshot_config(self) -> None:
        """Check and write pass an explicit StyLua config rooted in their source tree."""
        self.write(".stylua.toml", "column_width = 100\n")
        self.git("add", ".stylua.toml")
        self.git("commit", "-qm", "stylua config")
        result = self.run_format("--all-tracked", "--formatter", "stylua")
        self.assertEqual(0, result.returncode, result.stderr)
        check_paths = self.stylua_config_paths()
        self.assertEqual(1, len(check_paths), self.calls())
        self.assertTrue(check_paths[0].endswith("/.stylua.toml"), check_paths)
        self.assertNotEqual(self.repo.resolve(), Path(check_paths[0]).parent.resolve())
        self.log.unlink()
        result = self.run_format("--all-tracked", "--formatter", "stylua", "--write")
        self.assertEqual(0, result.returncode, result.stderr)
        write_paths = self.stylua_config_paths()
        self.assertEqual(1, len(write_paths), self.calls())
        self.assertEqual(self.repo.resolve(), Path(write_paths[0]).parent.resolve())

    def test_working_tree_source_checks_unstaged_sources(self) -> None:
        """The working-tree source validates unstaged edits and leaves the index untouched."""
        path = "lua/magit-lsp/ok.lua"
        self.write(path, "bad\n")
        result = self.run_format("--all-tracked", "--formatter", "stylua")
        self.assertEqual(0, result.returncode, result.stderr)
        result = self.run_format("--all-tracked", "--source", "working-tree", "--formatter", "stylua")
        self.assertEqual(1, result.returncode)
        self.assertEqual("bad\n", self.read(path))
        self.assertEqual("ok\n", self.git("show", f":{path}"))
        self.write(path, "ok\n")
        result = self.run_format("--all-tracked", "--source", "working-tree", "--formatter", "stylua")
        self.assertEqual(0, result.returncode, result.stderr)


class CIWorkflowEnvironmentTest(unittest.TestCase):
    """Guard the Linux CI jobs' locked flake-shell contract."""

    def workflow(self) -> str:
        """Return the tracked CI workflow source."""
        return CI_WORKFLOW.read_text()

    def test_linux_jobs_use_locked_flake_shell(self) -> None:
        """Every Linux job runs its tools inside the pinned dev shell."""
        violations = workflow_environment_violations(self.workflow())
        self.assertEqual([], violations)
        lock = (REPO / "flake.lock").read_text()
        self.assertIn('"locked"', lock)
        self.assertIn('"narHash"', lock)

    def test_missing_nix_coverage_is_rejected(self) -> None:
        """A coverage command outside the shell is reported."""
        workflow = self.workflow().replace(
            f"{NIX_DEVELOP} make coverage-rust", "make coverage-rust"
        )
        violations = workflow_environment_violations(workflow)
        self.assertTrue(any("coverage:" in v for v in violations), violations)

    def test_missing_nix_format_execution_is_rejected(self) -> None:
        """A formatter run outside the shell and an ad hoc install are reported."""
        workflow = self.workflow().replace(
            f'{NIX_DEVELOP} python3 scripts/format.py --base "$base"',
            "cargo install stylua --version 2.6.0 --locked\n"
            '          python3 scripts/format.py --base "$base"',
        )
        violations = workflow_environment_violations(workflow)
        self.assertTrue(any("format-check:" in v for v in violations), violations)
        self.assertTrue(any("cargo install" in v for v in violations), violations)


if __name__ == "__main__":
    unittest.main()
