"""End-to-end safety tests for opt-in formatter hooks."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
STYLUA = ROOT / "clients/vscode/node_modules/.bin/stylua"
PRETTIER = ROOT / "clients/vscode/node_modules/.bin/prettier"


class HookIntegration(unittest.TestCase):
    """Exercise installation and real pre-commit execution in disposable repos."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp.name) / "repo"
        self.repo.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "hooks@example.test")
        self.git("config", "user.name", "Hook Test")
        self.git("commit", "--allow-empty", "-qm", "base")
        (self.repo / ".githooks").mkdir()
        shutil.copy(ROOT / ".githooks/pre-commit", self.repo / ".githooks/pre-commit")
        (self.repo / "scripts").mkdir()
        shutil.copy(ROOT / "scripts/format.py", self.repo / "scripts/format.py")
        # Formatter runner locates pinned tools and configs relative to its own checkout.
        self.git("config", "core.hooksPath", ".githooks")
        (self.repo / ".githooks/pre-commit").chmod(0o755)
        if STYLUA.exists() and PRETTIER.exists():
            modules = self.repo / "clients/vscode/node_modules"
            modules.parent.mkdir(parents=True, exist_ok=True)
            modules.symlink_to(ROOT / "clients/vscode/node_modules", target_is_directory=True)
            shutil.copy(ROOT / ".stylua.toml", self.repo / ".stylua.toml")
            shutil.copy(ROOT / ".prettierrc.json", self.repo / ".prettierrc.json")

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args, check=True, env=None):
        return subprocess.run(["git", *args], cwd=self.repo, check=check, env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

    def install(self, action="install"):
        return subprocess.run(["sh", str(ROOT / "scripts/install-hooks.sh"), action], cwd=self.repo,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

    def source(self, path, text):
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
        self.git("add", path)

    def test_installer_idempotent_remove_and_external_conflicts(self):
        self.git("config", "--unset", "core.hooksPath")
        self.assertEqual(self.install().returncode, 0)
        self.assertEqual(self.install().returncode, 0)
        self.assertEqual(self.git("config", "--get", "core.hooksPath").stdout.strip(), ".githooks")
        self.assertEqual(self.install("remove").returncode, 0)
        self.assertNotEqual(self.git("config", "--get", "core.hooksPath", check=False).returncode, 0)

        self.git("config", "core.hooksPath", "custom-hooks")
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing existing core.hooksPath", result.stdout)
        self.assertEqual(self.git("config", "--get", "core.hooksPath").stdout.strip(), "custom-hooks")

    def test_refuses_custom_default_hook_without_overwriting(self):
        self.git("config", "--unset", "core.hooksPath")
        hook = self.repo / ".git/hooks/pre-commit"
        hook.write_text("#!/bin/sh\necho custom\n")
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing existing .git/hooks/pre-commit", result.stdout)
        self.assertEqual(hook.read_text(), "#!/bin/sh\necho custom\n")

    @unittest.skipUnless(STYLUA.exists() and PRETTIER.exists(), "locked formatters are not installed")
    def test_actual_commits_reject_bad_and_allow_formatted_lua_and_web(self):
        for path, bad, good in (
            ("lua/a.lua", 'local x="bad"\n', 'local x = "good"\n'),
            ("clients/a.ts", "const x:number=1\n", "const x: number = 1;\n"),
            ("clients/a.js", "const x={a:1}\n", "const x = { a: 1 };\n"),
        ):
            self.source(path, bad)
            indexed = self.git("show", f":{path}").stdout
            failed = self.git("commit", "-m", "bad", check=False)
            self.assertNotEqual(failed.returncode, 0, failed.stdout)
            self.assertEqual(self.git("show", f":{path}").stdout, indexed)
            self.assertEqual((self.repo / path).read_text(), bad)
            self.source(path, good)
            passed = self.git("commit", "-m", "formatted", check=False)
            self.assertEqual(passed.returncode, 0, passed.stdout)

    def test_missing_formatter_gives_install_guidance(self):
        self.source("lua/a.lua", 'local x = "ok"\n')
        (self.repo / "clients/vscode/node_modules/.bin/stylua").unlink()
        result = self.git("commit", "-m", "missing", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("install locked VSCode dependencies with npm ci in clients/vscode", result.stdout)


if __name__ == "__main__":
    unittest.main()
