#!/usr/bin/env python3
"""Test the Makefile ``run`` macro through temporary included Makefiles.

Each test writes a throwaway Makefile next to an isolated ``build/test-logs``
tree. The throwaway Makefile *includes* the real repository Makefile so the
``run`` macro under test is the production one, never a reimplementation. A
target then invokes ``$(call run,...)`` with a command that succeeds quietly,
emits warnings, or fails, and the test asserts on the macro's output and on the
log files it keeps or removes. Run via ``make wrapper-check``.
"""
from __future__ import annotations

import glob
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO_MAKEFILE = str(Path(__file__).resolve().parent.parent / 'Makefile')

# Recipe bodies: each target calls the real macro with a fixed, predictable
# command. ``sh -c`` keeps multi-command scripts as a single ``$(2)`` argument
# so the macro's redirection captures all of their stdout and stderr.
WRAPPER_TARGETS = {
    'wrap-ok': "printf 'no warnings here\\n'",
    'wrap-incidental': "printf 'all tests passed; 0 warnings; warning count zero\\n'",
    'wrap-warn': (
        'sh -c \'printf "stdout line\\n"; '
        'printf "warning: something unused\\n" >&2\''
    ),
    'wrap-warn-c': (
        'sh -c \'printf "src/main.c:12:3: warning: unused variable\\n"\''
    ),
    'wrap-warn-py': (
        'sh -c \'printf "scripts/foo.py:42: UserWarning: deprecated\\n"\''
    ),
    'wrap-fail': (
        'sh -c \'printf "line 1\\n"; printf "line 2\\n"; '
        'printf "line 3\\n"; printf "line 4\\n"; '
        'printf "line 5\\n"; exit 7\''
    ),
}


class MakeWrapperTest(unittest.TestCase):
    """Exercise the production ``run`` macro via temporary Makefiles."""

    def make_project(self) -> str:
        """Return a temp dir with an isolated log tree and a test Makefile."""
        directory = tempfile.mkdtemp(prefix='magit-lsp-wrap-')
        self.addCleanup(shutil.rmtree, directory, ignore_errors=True)
        os.makedirs(os.path.join(directory, 'build', 'test-logs'), exist_ok=True)
        lines = [f'include {REPO_MAKEFILE}\n', '\n',
                 '.PHONY: ' + ' '.join(WRAPPER_TARGETS) + '\n', '\n']
        for name, command in WRAPPER_TARGETS.items():
            lines.append(f'{name}:\n\t$(call run,{name},{command})\n\n')
        Path(directory, 'Makefile').write_text(''.join(lines))
        return directory

    def run_make(self, directory: str, target: str, *variables: str):
        """Run one wrapper target in the temp project; return CompletedProcess."""
        return subprocess.run(
            ['make', '-f', 'Makefile', target, *variables],
            cwd=directory,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )

    def logs(self, directory: str, name: str):
        """Return the log files currently retained for a wrapper target."""
        return sorted(glob.glob(os.path.join(
            directory, 'build', 'test-logs', f'{name}.*')))

    def log_text(self, path: str) -> str:
        """Read a retained log file in full."""
        with open(path) as handle:
            return handle.read()

    def test_success_with_warning_retains_log_and_notifies(self):
        """A1: warning success exits 0, notifies, and keeps full stdout/stderr."""
        directory = self.make_project()
        result = self.run_make(directory, 'wrap-warn')
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn('ok: wrap-warn', result.stdout)
        self.assertIn('warnings detected', result.stdout)
        self.assertIn('check log:', result.stdout)
        logs = self.logs(directory, 'wrap-warn')
        self.assertEqual(len(logs), 1, result.stdout)
        self.assertIn(os.path.relpath(logs[0], directory), result.stdout)
        content = self.log_text(logs[0])
        self.assertIn('stdout line', content)
        self.assertIn('warning: something unused', content)

    def test_compiler_and_python_warning_lines_are_detected(self):
        """A1: structural compiler and Python warning lines retain their logs."""
        directory = self.make_project()
        for target in ('wrap-warn-c', 'wrap-warn-py'):
            with self.subTest(target=target):
                result = self.run_make(directory, target)
                self.assertEqual(result.returncode, 0, result.stdout)
                self.assertIn('warnings detected', result.stdout)
                self.assertEqual(len(self.logs(directory, target)), 1,
                                 result.stdout)

    def test_success_without_warning_is_concise_and_removes_log(self):
        """A2: warning-free success prints only `ok:` and deletes its log."""
        directory = self.make_project()
        result = self.run_make(directory, 'wrap-ok')
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn('ok: wrap-ok', result.stdout)
        self.assertNotIn('warnings detected', result.stdout)
        self.assertEqual(self.logs(directory, 'wrap-ok'), [], result.stdout)

    def test_incidental_warning_prose_is_not_detected(self):
        """A2: prose containing 'warning' is not treated as a diagnostic."""
        directory = self.make_project()
        result = self.run_make(directory, 'wrap-incidental')
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertNotIn('warnings detected', result.stdout)
        self.assertEqual(self.logs(directory, 'wrap-incidental'), [],
                         result.stdout)

    def test_failure_retains_log_with_exit_and_tail(self):
        """A3: failure keeps the full log, reports path/tail, and exits nonzero."""
        directory = self.make_project()
        result = self.run_make(directory, 'wrap-fail', 'LOG_TAIL=2')
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn('Error 7', result.stdout)
        self.assertIn('FAILED: wrap-fail', result.stdout)
        logs = self.logs(directory, 'wrap-fail')
        self.assertEqual(len(logs), 1, result.stdout)
        self.assertIn(os.path.relpath(logs[0], directory), result.stdout)
        content = self.log_text(logs[0])
        for number in range(1, 6):
            self.assertIn(f'line {number}', content)
        self.assertIn('line 4', result.stdout)
        self.assertIn('line 5', result.stdout)
        self.assertNotIn('line 1', result.stdout)

    def test_V1_streams_directly(self):
        """A3: V=1 bypasses logging for both failing and warning commands."""
        directory = self.make_project()
        failing = self.run_make(directory, 'wrap-fail', 'V=1')
        self.assertNotEqual(failing.returncode, 0, failing.stdout)
        self.assertIn('Error 7', failing.stdout)
        self.assertIn('line 1', failing.stdout)
        self.assertNotIn('FAILED: wrap-fail', failing.stdout)
        self.assertEqual(self.logs(directory, 'wrap-fail'), [], failing.stdout)
        warning = self.run_make(directory, 'wrap-warn', 'V=1')
        self.assertEqual(warning.returncode, 0, warning.stdout)
        self.assertIn('warning: something unused', warning.stdout)
        self.assertEqual(self.logs(directory, 'wrap-warn'), [], warning.stdout)


if __name__ == '__main__':
    unittest.main()
