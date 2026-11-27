#!/usr/bin/env python3
"""Exercise ``scripts/profile_ci_test.py`` around dummy passing and failing commands.

Run via ``make profiler-check``. Each case runs the real profiler (never a
reimplementation) in a temporary output directory, then asserts that the raw
samples and ``summary.json`` exist, that the profiler wrote nothing of its own
to the wrapped command's streams, that it propagated the wrapped command's exit
status unchanged and that both host-verdict collectors recorded a status. The
collector helpers are also unit-tested in-process, so no macOS command is run.
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
PROFILER = str(SCRIPTS / 'profile_ci_test.py')
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import profile_ci_test  # noqa: E402  (resolved through the path above)


def run_case(command, expected_code, expected_stdout, expected_stderr='',
             expected_victims=()):
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
        if result.stdout != expected_stdout or result.stderr != expected_stderr:
            raise SystemExit(
                f'FAILED: profiler altered the child streams: '
                f'stdout={result.stdout!r} stderr={result.stderr!r}')
        samples = Path(directory) / 'samples.log'
        summary_path = Path(directory) / 'summary.json'
        if not samples.is_file() or samples.stat().st_size == 0:
            raise SystemExit('FAILED: samples.log is missing or empty')
        if not (Path(directory) / 'child-output.log').is_file():
            raise SystemExit('FAILED: child-output.log is missing')
        captured = (Path(directory) / 'child-output.log').read_text(
            encoding='utf-8', errors='replace')
        if expected_stdout not in captured or expected_stderr not in captured:
            raise SystemExit('FAILED: child-output.log lost the child streams')
        summary = json.loads(summary_path.read_text(encoding='utf-8'))
        if summary['exit_code'] != expected_code:
            raise SystemExit(f'FAILED: summary exit_code {summary["exit_code"]!r}')
        if summary['sample_count'] < 1:
            raise SystemExit('FAILED: no samples were counted')
        if summary['command'] != command:
            raise SystemExit(f'FAILED: summary command {summary["command"]!r}')
        if 'status' not in summary['eslogger']:
            raise SystemExit('FAILED: summary omits the eslogger status')
        for key in ('ips_reports', 'kernel_log'):
            collector = summary.get(key)
            if not isinstance(collector, dict) or 'status' not in collector:
                raise SystemExit(f'FAILED: summary omits the {key} status')
            if collector['status'] == 'not-run':
                raise SystemExit(f'FAILED: {key} was never collected')
            if collector['victim_pids'] != sorted(expected_victims):
                raise SystemExit(
                    f'FAILED: {key} victim pids {collector["victim_pids"]!r}')
        if sys.platform != 'darwin':
            for key in ('ips_reports', 'kernel_log'):
                if summary[key]['status'] != 'unavailable':
                    raise SystemExit(
                        f'FAILED: {key} should be unavailable off Darwin: '
                        f'{summary[key]!r}')


def check_victim_pid_parsing():
    """Extract killed pids from the server's reap lines and ignore the rest."""
    text = '\n'.join((
        'child spawn pid=20793 reason="spawn" program=git',
        'child reap pid=19221 reason="reap" signal=9',
        'child reap pid=19222 reason="reap" signal=9',
        'child reap pid=19221 reason="reap" signal=9',
        'child reap pid=300 reason="reap" signal=15',
        'child term pid=400 reason="term"',
    ))
    if profile_ci_test.parse_victim_pids(text) != [19221, 19222]:
        raise SystemExit('FAILED: victim pid extraction')
    if profile_ci_test.parse_victim_pids('') != []:
        raise SystemExit('FAILED: empty output produced victim pids')


def check_log_window_and_predicate():
    """Window formatting and the kernel-only fallback need no macOS host."""
    started = datetime(2026, 11, 26, 22, 35, 40)
    finished = datetime(2026, 11, 26, 22, 45, 0)
    window = profile_ci_test.log_window(started, finished)
    if window != ('2026-11-26 22:35:40', '2026-11-26 22:45:00'):
        raise SystemExit(f'FAILED: log window {window!r}')
    if profile_ci_test.log_window(finished, started) != (
            '2026-11-26 22:45:00', '2026-11-26 22:45:00'):
        raise SystemExit('FAILED: a reversed window was not clamped')
    if profile_ci_test.kernel_predicate([]) != 'sender == "kernel"':
        raise SystemExit('FAILED: the kernel-only predicate is missing')
    predicate = profile_ci_test.kernel_predicate([19221])
    if predicate != 'sender == "kernel" OR eventMessage CONTAINS[c] "19221"':
        raise SystemExit(f'FAILED: victim predicate {predicate!r}')


def check_missing_diagnostic_reports():
    """A Darwin host without the report directories records absence, never raises."""
    with tempfile.TemporaryDirectory(prefix='profiler-check-') as directory:
        started = datetime.now()
        with open(Path(directory) / 'profiler.log', 'a', encoding='utf-8') as log:
            status = profile_ci_test.collect_ips_reports(
                Path(directory), [], started,
                started + timedelta(seconds=1), log, platform='darwin')
        probed = status['directories']
        if len(probed) != len(profile_ci_test.IPS_DIRECTORIES):
            raise SystemExit('FAILED: not every report directory was probed')
        if all(entry['status'] == 'absent' for entry in probed):
            if status['status'] != 'absent':
                raise SystemExit(
                    f'FAILED: absent directories reported {status["status"]!r}')


def main():
    """Run the profiler cases and host-verdict unit checks, then report."""
    run_case([sys.executable, '-c', 'print("profiler-check ok")'], 0,
             'profiler-check ok\n')
    run_case([sys.executable, '-c', 'import sys; sys.exit(3)'], 3, '')
    run_case([sys.executable, '-c', 'import sys; print("out"); '
              'print("err", file=sys.stderr); sys.exit(4)'], 4, 'out\n', 'err\n')
    run_case([sys.executable, '-c',
              'print(\'child reap pid=19221 reason="reap" signal=9\')'], 0,
             'child reap pid=19221 reason="reap" signal=9\n',
             expected_victims=(19221,))
    run_case([sys.executable, '-c',
              'import sys; sys.stdout.write("x" * 200000)'], 0, 'x' * 200000)
    check_victim_pid_parsing()
    check_log_window_and_predicate()
    check_missing_diagnostic_reports()
    print('ok: profiler-check preserved the child streams/exit status, recorded '
          'both host-verdict collectors and parsed victim pids')
    return 0


if __name__ == '__main__':
    sys.exit(main())
