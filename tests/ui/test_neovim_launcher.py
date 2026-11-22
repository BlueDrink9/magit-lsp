"""Unit tests for the interactive `make neovim` launcher (NVIM-LAUNCH).

The launcher is tested through its process adapter: no test starts Neovim or
the server. The dedicated initializer is checked as a stable contract so a test
run can never silently fall back to the MiniTest initializer or user config.
"""

from __future__ import annotations

import importlib.util
import io
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from importlib.machinery import SourceFileLoader
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
LAUNCHER_PATH = ROOT / "scripts" / "neovim"
INIT_PATH = ROOT / "scripts" / "neovim_init.lua"


def load_launcher() -> object:
    """Import the extensionless launcher script as a module."""
    loader = SourceFileLoader("magit_neovim_launcher", str(LAUNCHER_PATH))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot import {LAUNCHER_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


launcher = load_launcher()

_DEFAULT_REPO = object()


class Recorded:
    """One fake Neovim invocation and the filesystem state it observed."""

    def __init__(self, command: list[str], options: dict[str, object]) -> None:
        self.command = command
        self.options = options
        self.environment = dict(options["env"])  # type: ignore[arg-type]
        self.base = Path(self.environment["MAGIT_LSP_LAUNCH_DIR"])
        self.base_existed = self.base.is_dir()
        # Snapshot the ShaDa path and Git include file while the isolated
        # directories still exist.
        self.shada = Path(command[command.index("-i") + 1])
        self.shada_parent_existed = self.shada.parent.is_dir()
        self.shada_is_directory = self.shada.is_dir()
        self.gitconfig = self.base / launcher.GIT_GLOBAL_CONFIG  # type: ignore[attr-defined]
        self.gitconfig_existed = self.gitconfig.is_file()
        self.gitconfig_content = (
            self.gitconfig.read_text(encoding="utf-8")
            if self.gitconfig_existed
            else None
        )

    @property
    def cwd(self) -> str:
        """The working directory passed to the editor process."""
        return str(self.options["cwd"])

    def isolated_paths(self) -> list[str]:
        """Every isolated state variable value."""
        return [
            self.environment[name]
            for name in (
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_STATE_HOME",
                "XDG_CACHE_HOME",
                "XDG_RUNTIME_DIR",
            )
        ]


class LauncherTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo with spaces"
        self.repo.mkdir()
        self.server = self.root / "server with spaces"
        self.server.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        self.server.chmod(0o755)
        self.temporary_root = self.root / "tmp"
        self.temporary_root.mkdir()
        self.recorded: list[Recorded] = []

    def fake_runner(self, status: int = 0, error: OSError | None = None):
        """Return a process adapter that records one invocation."""

        def runner(command: list[str], **options: object) -> subprocess.CompletedProcess[str]:
            self.recorded.append(Recorded(list(command), dict(options)))
            if error is not None:
                raise error
            return subprocess.CompletedProcess(command, status)

        return runner

    def run_launcher(
        self,
        *,
        status: int = 0,
        error: OSError | None = None,
        repo: object = _DEFAULT_REPO,
        **kwargs: object,
    ):
        """Launch against the fake infrastructure and return the exit status."""
        parameters = {
            "nvim": "/fake/nvim",
            "server": self.server,
            "runner": self.fake_runner(status, error),
            "inherited": {"PATH": "/usr/bin:/bin", "TERM": "xterm-256color"},
            "temporary_root": self.temporary_root,
        }
        parameters.update(kwargs)
        selected = self.repo if repo is _DEFAULT_REPO else repo
        return launcher.launch(selected, **parameters)  # type: ignore[attr-defined]

    def test_argv_uses_the_dedicated_initializer_without_user_config(self) -> None:
        self.assertEqual(self.run_launcher(), 0)
        (call,) = self.recorded
        self.assertEqual(call.command[0], "/fake/nvim")
        self.assertIn("--noplugin", call.command)
        self.assertEqual(
            call.command[call.command.index("-u") + 1], str(INIT_PATH)
        )
        shada = Path(call.command[call.command.index("-i") + 1])
        self.assertEqual(shada, call.shada)
        self.assertEqual(shada.name, "main.shada")
        self.assertEqual(shada.parent, call.base / "shada")
        self.assertTrue(shada.is_relative_to(call.base))
        self.assertTrue(call.shada_parent_existed)
        self.assertFalse(call.shada_is_directory)
        self.assertTrue(call.base_existed)
        self.assertEqual(call.command.count("-u"), 1)
        self.assertNotIn("-c", call.command)
        self.assertNotIn("--cmd", call.command)
        self.assertNotIn("minimal_init", " ".join(call.command))

    def test_environment_keeps_caller_home_and_isolates_editor_state(self) -> None:
        self.run_launcher(inherited={
            "PATH": "/usr/bin:/bin",
            "TERM": "screen-256color",
            "HOME": "/home/someone",
            "XDG_CONFIG_HOME": "/home/someone/.config",
        })
        (call,) = self.recorded
        self.assertEqual(call.environment["HOME"], "/home/someone")
        for value in call.isolated_paths():
            self.assertTrue(
                value.startswith(str(call.base) + os.sep), value
            )
        self.assertNotEqual(
            call.environment["XDG_CONFIG_HOME"], "/home/someone/.config"
        )
        self.assertEqual(call.environment["PATH"], "/usr/bin:/bin")
        self.assertEqual(call.environment["TERM"], "screen-256color")
        self.assertEqual(call.environment["MAGIT_LSP_BIN"], str(self.server))
        self.assertEqual(
            call.environment["MAGIT_LSP_LAUNCH_REPO"], str(self.repo)
        )
        self.assertEqual(
            call.environment["MAGIT_LSP_LAUNCH_DIR"], str(call.base)
        )
        self.assertEqual(
            call.environment["GIT_CONFIG_GLOBAL"], str(call.gitconfig)
        )
        self.assertTrue(call.gitconfig_existed)
        self.assertIn("--noplugin", call.command)

    def test_global_git_config_includes_original_xdg_then_home(self) -> None:
        home = self.root / "caller home"
        xdg = self.root / "caller xdg"
        (xdg / "git").mkdir(parents=True)
        (xdg / "git" / "config").write_text(
            "[user]\n\tname = XDG Sentinel\n", encoding="utf-8"
        )
        home.mkdir()
        (home / ".gitconfig").write_text(
            "[user]\n\temail = home-sentinel@example.com\n", encoding="utf-8"
        )
        self.run_launcher(inherited={
            "PATH": "/usr/bin:/bin",
            "TERM": "xterm-256color",
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(xdg),
        })
        (call,) = self.recorded
        self.assertEqual(
            call.environment["GIT_CONFIG_GLOBAL"], str(call.gitconfig)
        )
        text = call.gitconfig_content or ""
        self.assertTrue(text)
        xdg_include = str(xdg / "git" / "config")
        home_include = str(home / ".gitconfig")
        self.assertIn(f'path = "{xdg_include}"', text)
        self.assertIn(f'path = "{home_include}"', text)
        self.assertLess(text.index(xdg_include), text.index(home_include))
        # Include-only: caller values are never copied into the temporary file.
        self.assertNotIn("XDG Sentinel", text)
        self.assertNotIn("home-sentinel@example.com", text)
        self.assertEqual(text.count("[include]"), 2)

    def test_home_config_uses_default_xdg_location_when_unset(self) -> None:
        home = self.root / "home-only"
        home.mkdir()
        self.run_launcher(inherited={
            "PATH": "/usr/bin:/bin",
            "TERM": "xterm-256color",
            "HOME": str(home),
        })
        (call,) = self.recorded
        text = call.gitconfig_content or ""
        self.assertTrue(text)
        default_xdg = str(home / ".config" / "git" / "config")
        home_include = str(home / ".gitconfig")
        self.assertIn(f'path = "{default_xdg}"', text)
        self.assertLess(text.index(default_xdg), text.index(home_include))

    def test_explicit_git_config_global_is_not_replaced(self) -> None:
        custom = self.root / "custom global"
        self.run_launcher(inherited={
            "PATH": "/usr/bin:/bin",
            "TERM": "xterm-256color",
            "HOME": "/home/someone",
            "GIT_CONFIG_GLOBAL": str(custom),
        })
        (call,) = self.recorded
        self.assertEqual(call.environment["GIT_CONFIG_GLOBAL"], str(custom))
        self.assertFalse(call.gitconfig_existed)

    def test_empty_git_config_global_override_is_preserved(self) -> None:
        self.run_launcher(inherited={
            "PATH": "/usr/bin:/bin",
            "TERM": "xterm-256color",
            "HOME": "/home/someone",
            "GIT_CONFIG_GLOBAL": "",
        })
        (call,) = self.recorded
        self.assertEqual(call.environment["GIT_CONFIG_GLOBAL"], "")
        self.assertFalse(call.gitconfig_existed)

    def test_chosen_repository_with_spaces_is_passed_verbatim(self) -> None:
        self.run_launcher()
        (call,) = self.recorded
        self.assertEqual(call.environment["MAGIT_LSP_LAUNCH_REPO"], str(self.repo))
        self.assertEqual(call.cwd, str(self.repo))

    def test_default_repository_is_the_working_directory(self) -> None:
        self.run_launcher(cwd=str(self.repo), repo=None)
        (call,) = self.recorded
        self.assertEqual(call.environment["MAGIT_LSP_LAUNCH_REPO"], str(self.repo))
        self.assertEqual(call.cwd, str(self.repo))
        self.assertEqual(launcher.repository(None, cwd=str(self.repo)), self.repo)  # type: ignore[attr-defined]

    def test_launcher_defaults_are_the_checkout_server_and_client_runtime(self) -> None:
        self.assertEqual(
            launcher.SERVER, ROOT / "target" / "debug" / "magit-lsp"  # type: ignore[attr-defined]
        )
        self.assertEqual(
            launcher.CLIENT_RUNTIME, ROOT / "clients" / "neovim"  # type: ignore[attr-defined]
        )
        self.assertTrue(launcher.CLIENT_RUNTIME.is_dir())  # type: ignore[attr-defined]
        self.run_launcher()
        (call,) = self.recorded
        self.assertEqual(call.environment["MAGIT_LSP_BIN"], str(self.server))

    def test_temporary_directory_is_removed_after_exit(self) -> None:
        self.run_launcher(inherited={
            "PATH": "/usr/bin:/bin",
            "TERM": "xterm-256color",
            "HOME": "/home/someone",
        })
        (call,) = self.recorded
        self.assertTrue(call.base_existed)
        self.assertTrue(call.gitconfig_existed)
        self.assertFalse(call.base.exists())
        self.assertFalse(call.gitconfig.exists())
        self.assertTrue(call.base.is_relative_to(self.temporary_root))

    def test_exit_status_is_propagated(self) -> None:
        self.assertEqual(self.run_launcher(status=23), 23)
        (call,) = self.recorded
        self.assertFalse(call.base.exists())

    def test_missing_server_fails_before_creating_isolated_state(self) -> None:
        with self.assertRaises(launcher.LauncherError):  # type: ignore[attr-defined]
            self.run_launcher(server=self.root / "missing-server")
        self.assertEqual(self.recorded, [])
        self.assertEqual(list(self.temporary_root.iterdir()), [])

    def test_missing_repository_is_reported(self) -> None:
        with self.assertRaises(launcher.LauncherError):  # type: ignore[attr-defined]
            self.run_launcher(repo=self.root / "missing-repo")
        self.assertEqual(self.recorded, [])

    def test_process_failure_cleans_up_and_propagates(self) -> None:
        with self.assertRaises(OSError):
            self.run_launcher(error=OSError("no terminal"))
        (call,) = self.recorded
        self.assertFalse(call.base.exists())

    def test_main_reports_launcher_errors_without_a_traceback(self) -> None:
        stderr = io.StringIO()
        with patch.object(
            launcher,
            "launch",
            side_effect=launcher.LauncherError("server binary is not executable"),
        ):
            with redirect_stderr(stderr):
                status = launcher.main(["--repo", str(self.repo)])
        self.assertEqual(status, 2)
        self.assertIn("neovim launcher: server binary is not executable", stderr.getvalue())


class InitializerContractTest(unittest.TestCase):
    def test_initializer_loads_the_nested_client_and_opens_the_selected_repo(self) -> None:
        text = INIT_PATH.read_text(encoding="utf-8")
        self.assertNotIn("mini.nvim", text)
        self.assertNotIn("mini.test", text)
        self.assertIn('project_root .. "/clients/neovim"', text)
        self.assertIn("vim.opt.runtimepath = runtimepath", text)
        self.assertIn("vim.opt.packpath = {}", text)
        self.assertIn("vim.env.VIMRUNTIME", text)
        self.assertIn('client_runtime .. "/plugin/magit-lsp.lua"', text)
        self.assertIn("MAGIT_LSP_LAUNCH_REPO", text)
        self.assertIn('require("magit-lsp").open(', text)
        self.assertNotIn("setup(", text)

    def test_launcher_selects_only_the_dedicated_initializer(self) -> None:
        init = launcher.INIT  # type: ignore[attr-defined]
        self.assertEqual(init, INIT_PATH)
        self.assertNotEqual(init.name, "minimal_init.lua")
        minimum = (ROOT / "scripts" / "minimal_init.lua").read_text(encoding="utf-8")
        self.assertNotIn(init.name, minimum)
        self.assertNotIn("neovim_init", minimum)


if __name__ == "__main__":
    unittest.main()
