#!/usr/bin/env python3
"""Test the pre-commit staged autofix and its working-copy formatting.

Each test builds a throwaway Git repository with tracked and generator-owned
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
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FORMAT_PY = REPO / "scripts/format.py"
HOOK = REPO / ".githooks/pre-commit"
CI_WORKFLOW = REPO / ".github/workflows/ci.yml"
NIX_DEVELOP = "nix develop --no-update-lock-file -c"
NATIVE_JOB = "native-test"
NATIVE_POLICY = "continue-on-error: ${{ matrix.informational }}"

# Every Linux job must run its tool commands inside the flake's locked dev
# shell, and it must not provision any of these tools itself.
LINUX_JOB_COMMANDS = {
    "format-check": (
        f"{NIX_DEVELOP} make vscode-deps",
        f'{NIX_DEVELOP} python3 scripts/format.py --base "$base"',
    ),
    "release-contract": (f"{NIX_DEVELOP} make release-contract-check",),
    "test": (
        f"{NIX_DEVELOP} make test-rust",
        f"{NIX_DEVELOP} make vscode-test",
        f"{NIX_DEVELOP} make test-lua",
    ),
    "coverage": (f"{NIX_DEVELOP} make coverage-rust",),
    "documentation": (f"{NIX_DEVELOP} make docs-artifacts-check docs-check",),
    "ui-test": (
        f"{NIX_DEVELOP} make build",
        "nix develop .#vscode --no-update-lock-file -c env CI=1 make ui-unit ui-test",
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
    "clients/neovim/lua/magit-lsp/types.lua",
    "clients/neovim/lua/magit-lsp/shared.lua",
    "clients/vscode/types/protocol.d.ts",
    "clients/vscode/types/shared-lists.js",
)
GENERATOR_NEIGHBORS = (
    "clients/neovim/lua/magit-lsp/handwritten.lua",
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


def native_lane_entries(workflow: str) -> list[dict[str, str]]:
    """Return the native-test matrix include entries as field dictionaries."""
    body = workflow_jobs(workflow).get(NATIVE_JOB, "")
    try:
        start = body.index("include:")
    except ValueError:
        return []
    section = body[start:]
    steps = section.find("\n    steps:")
    if steps != -1:
        section = section[:steps]
    lanes: list[dict[str, str]] = []
    current = None
    for line in section.splitlines():
        stripped = line.strip()
        entry = re.match(r"^- os:\s*(\S+)$", stripped)
        if entry:
            current = {"os": entry.group(1)}
            lanes.append(current)
            continue
        field = re.match(r"^([a-z][a-z-]*):\s*(\S+)$", stripped)
        if field and current is not None:
            current[field.group(1)] = field.group(2)
    return lanes


def native_test_policy_violations(workflow: str) -> list[str]:
    """Report native lanes that are not informational on Windows only.

    Windows support is deferred, so its lane must not gate. macOS lanes remain
    required, Linux jobs stay gating and nothing may depend on the native job.
    """
    jobs = workflow_jobs(workflow)
    body = jobs.get(NATIVE_JOB)
    if body is None:
        return [f"{NATIVE_JOB}: job is missing"]
    violations = []
    lines = [line.strip() for line in body.splitlines()]
    if NATIVE_POLICY not in lines:
        violations.append(
            f"{NATIVE_JOB}: must gate failures only with '{NATIVE_POLICY}'"
        )
    for line in lines:
        if line.startswith("continue-on-error:") and line != NATIVE_POLICY:
            violations.append(f"{NATIVE_JOB}: unexpected '{line}'")
    lanes = native_lane_entries(workflow)
    windows = [lane for lane in lanes if lane["os"].startswith("windows-")]
    required = [lane for lane in lanes if not lane["os"].startswith("windows-")]
    if not windows:
        violations.append(f"{NATIVE_JOB}: Windows lane is missing")
    for lane in windows:
        if lane.get("informational") != "true":
            violations.append(
                f"{NATIVE_JOB}: {lane['os']} must set informational: true"
            )
    if not required:
        violations.append(f"{NATIVE_JOB}: non-Windows lanes are missing")
    for lane in required:
        if lane.get("informational") != "false":
            violations.append(
                f"{NATIVE_JOB}: {lane['os']} must set informational: false"
            )
    for name, other in jobs.items():
        if name == NATIVE_JOB:
            continue
        if re.search(rf"^\s*needs:.*\b{NATIVE_JOB}\b", other, re.M) or re.search(
            rf"^\s*-\s*{NATIVE_JOB}\s*$", other, re.M
        ):
            violations.append(f"{name}: must not depend on {NATIVE_JOB}")
        if re.search(r"^    continue-on-error:", other, re.M):
            violations.append(f"{name}: must remain a gating job")
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
        if not candidate.is_file() or "bad" in candidate.read_text():
            status = 1
    elif candidate.is_file():
        candidate.write_text(candidate.read_text().replace("bad", "ok"))
sys.exit(status)
'''

SEED = {
    "Cargo.toml": '[package]\nname = "fixture"\nversion = "0.0.0"\nedition = "2021"\n',
    "src/main.rs": "ok\n",
    "src/lib.rs": "ok\n",
    "clients/neovim/lua/magit-lsp/ok.lua": "ok\n",
    **{path: "ok\n" for path in GENERATOR_OWNED + GENERATOR_NEIGHBORS},
}


class FormatSweepTest(unittest.TestCase):
    """Exercise hook staged autofix and working-copy formatting."""

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

    def test_ordinary_hook_preserves_generator_owned_client_references(self) -> None:
        """The hook leaves generated bytes alone and still fixes eligible neighbors."""
        self.install_hook()
        generated = "clients/neovim/lua/magit-lsp/types.lua"
        handwritten = "clients/neovim/lua/magit-lsp/handwritten.lua"
        for path in (generated, handwritten):
            self.write(path, "bad\n")
            self.git("add", path)
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("bad\n", self.git("show", f":{generated}"))
        self.assertEqual("bad\n", self.read(generated))
        self.assertEqual("ok\n", self.git("show", f":{handwritten}"))
        self.assertEqual("ok\n", self.read(handwritten))
        joined = "\n".join(self.calls())
        self.assertIn(handwritten, joined)
        for path in GENERATOR_OWNED:
            self.assertNotIn(path, joined)

    def test_hook_formats_matching_working_copy(self) -> None:
        """A staged-only change formats the index and its matching working file."""
        self.install_hook()
        path = "clients/neovim/lua/magit-lsp/ok.lua"
        self.write(path, "bad\n")
        self.git("add", path)
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.git("show", f":{path}"))
        self.assertEqual(b"ok\n", (self.repo / path).read_bytes())
        self.assertEqual("", self.git("status", "--porcelain", "--untracked-files=no", "--", path))

    def test_hook_formats_divergent_working_copy_in_place(self) -> None:
        """Divergent working copies keep their own content and are formatted in place."""
        self.install_hook()
        changed = "clients/neovim/lua/magit-lsp/ok.lua"
        unchanged = "clients/neovim/lua/magit-lsp/handwritten.lua"
        self.write(unchanged, "old\n")
        self.git("add", unchanged)
        self.git("commit", "-qm", "prepare unchanged")
        self.write(changed, "bad\n")
        self.git("add", changed)
        self.write(changed, "bad\nunstaged\n")
        self.write(unchanged, "ok\n")
        self.git("add", unchanged)
        self.write(unchanged, "bad\n")
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.git("show", f":{changed}"))
        self.assertEqual("ok\nunstaged\n", self.read(changed))
        self.assertEqual("ok\n", self.git("show", f":{unchanged}"))
        self.assertEqual("ok\n", self.read(unchanged))
        joined = "\n".join(self.calls())
        self.assertIn(changed, joined)
        self.assertIn(unchanged, joined)

    def test_hook_skips_unwritable_working_copy(self) -> None:
        """An unwritable working file leaves the index formatted on both branches."""
        self.install_hook()
        matching = "clients/neovim/lua/magit-lsp/ok.lua"
        divergent = "clients/neovim/lua/magit-lsp/handwritten.lua"
        self.write(matching, "bad\n")
        self.write(divergent, "bad\n")
        self.git("add", matching, divergent)
        self.write(divergent, "bad\nunstaged\n")
        working = self.repo / matching
        other = self.repo / divergent
        for candidate in (working, other):
            candidate.chmod(0o444)
            self.addCleanup(candidate.chmod, 0o644)
        if os.access(working, os.W_OK) or os.access(other, os.W_OK):
            self.skipTest("fixture files stay writable (the suite is running as root)")
        result = self.run_hook()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("ok\n", self.git("show", f":{matching}"))
        self.assertEqual("ok\n", self.git("show", f":{divergent}"))
        self.assertEqual(b"bad\n", working.read_bytes())
        self.assertEqual(b"bad\nunstaged\n", other.read_bytes())
        self.assertIn(f"working file left unchanged: {matching}", result.stderr)
        self.assertIn(f"working file left unchanged: {divergent}", result.stderr)

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


class CIWorkflowEnvironmentTest(unittest.TestCase):
    """Guard the Linux flake-shell and native Windows informational contracts."""

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

    def test_windows_native_lane_is_informational(self) -> None:
        """Only the Windows lane may fail without failing the workflow."""
        violations = native_test_policy_violations(self.workflow())
        self.assertEqual([], violations)

    def test_missing_windows_informational_policy_is_rejected(self) -> None:
        """A Windows lane without the informational flag is reported."""
        workflow = self.workflow().replace("            informational: true\n", "")
        violations = native_test_policy_violations(workflow)
        self.assertTrue(any("windows-2025" in v for v in violations), violations)

    def test_optional_macos_lane_is_rejected(self) -> None:
        """A macOS lane made optional is reported."""
        workflow = self.workflow().replace(
            "            informational: false\n", "            informational: true\n", 1
        )
        violations = native_test_policy_violations(workflow)
        self.assertTrue(any("informational: false" in v for v in violations), violations)

    def test_optional_linux_job_is_rejected(self) -> None:
        """A Linux job made non-gating is reported."""
        workflow = self.workflow().replace(
            "  test:\n    runs-on: ubuntu-24.04",
            "  test:\n    continue-on-error: true\n    runs-on: ubuntu-24.04",
        )
        violations = native_test_policy_violations(workflow)
        self.assertTrue(any("test: must remain a gating job" in v for v in violations), violations)

    def test_native_test_dependency_is_rejected(self) -> None:
        """A job depending on the native matrix is reported."""
        workflow = self.workflow().replace(
            "  test:\n    runs-on: ubuntu-24.04",
            "  test:\n    needs: native-test\n    runs-on: ubuntu-24.04",
        )
        violations = native_test_policy_violations(workflow)
        self.assertTrue(any("test: must not depend on native-test" in v for v in violations), violations)

    def test_broad_native_continue_on_error_is_rejected(self) -> None:
        """A blanket continue-on-error replaces the per-lane policy."""
        workflow = self.workflow().replace(
            NATIVE_POLICY, "continue-on-error: true"
        )
        violations = native_test_policy_violations(workflow)
        self.assertTrue(any(NATIVE_POLICY in v for v in violations), violations)

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
