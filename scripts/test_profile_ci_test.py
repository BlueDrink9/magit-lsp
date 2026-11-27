#!/usr/bin/env python3
"""Exercise ``scripts/profile_ci_test.py`` around dummy passing and failing commands.

Run via ``make profiler-check``. Each case runs the real profiler (never a
reimplementation) in a temporary output directory, then asserts that the raw
samples and ``summary.json`` exist, that the profiler wrote nothing of its own
to the wrapped command's streams, and that it propagated the wrapped command's
exit status unchanged.
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

PROFILER = str(Path(__file__).resolve().parent / 'profile_ci_test.py')


def run_case(command, expected_code, expected_stdout):
    """Run one wrapped command and assert the profiler's observable contract."""
    with tempfile.TemporaryDirectory(prefix='profiler-check-') as directory:
        result = subprocess.run(
            [sys.executable, PROFILER, '--output-dir', directory,
             '--interval', '0.1', '--', *command],
            capture_output=True, text=True)
        if result.returncode != expected_code:
            raise SystemExit(
                f'FAILED: profiler exited {result.returncode}, expected the '
                f'child\'s {expected_code}: {result.stderr}')
        if result.stdout != expected_stdout or result.stderr:
            raise SystemExit(
                f'FAILED: profiler altered the child streams: '
                f'stdout={result.stdout!r} stderr={result.stderr!r}')
        samples = Path(directory) / 'samples.log'
        summary_path = Path(directory) / 'summary.json'
        if not samples.is_file() or samples.stat().st_size == 0:
            raise SystemExit('FAILED: samples.log is missing or empty')
        summary = json.loads(summary_path.read_text(encoding='utf-8'))
        if summary['exit_code'] != expected_code:
            raise SystemExit(f'FAILED: summary exit_code {summary["exit_code"]!r}')
        if summary['sample_count'] < 1:
            raise SystemExit('FAILED: no samples were counted')
        if summary['command'] != command:
            raise SystemExit(f'FAILED: summary command {summary["command"]!r}')
        if 'status' not in summary['eslogger']:
            raise SystemExit('FAILED: summary omits the eslogger status')


def main():
    """Run the passing and failing cases and print one success line."""
    run_case([sys.executable, '-c', 'print("profiler-check ok")'], 0,
             'profiler-check ok\n')
    run_case([sys.executable, '-c', 'import sys; sys.exit(3)'], 3, '')
    print('ok: profiler-check wrote samples + summary and propagated exit status')
    return 0


if __name__ == '__main__':
    sys.exit(main())
