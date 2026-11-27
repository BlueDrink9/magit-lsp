#!/usr/bin/env python3
"""Profile a wrapped CI test command while it runs on a macOS lane.

The hosted macOS runner kills short-lived helper children (the gated Git stubs)
with an unlogged `SIGKILL`, so a failing test otherwise names only a client-side
timeout. This wrapper runs the *same* locked Make invocation the Linux lanes run,
samples the test/server/Git process tree and host memory state once per interval,
and — where the host permits it — captures signal-sender events with
`eslogger signal`.

Everything the profiler itself does is best-effort and recorded in
`summary.json`, `samples.log` and `profiler.log` inside `--output-dir`: a missing
`eslogger`, a refused `sudo` or a failing sampler is noted, never fatal. The
profiler's exit status is always the wrapped command's, and the profiler writes
nothing to its own stdout/stderr, so a caller's existing log redirect keeps
exactly the bytes the test wrote.

Usage::

    scripts/profile_ci_test.py --output-dir "$RUNNER_TEMP/macos-profile" -- \\
        nix develop .#release-native -c make test-rust-artifact \\
        MAGIT_LSP_BIN="$binary" FILTER="--test server_async"
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
import traceback
from datetime import datetime, timezone
from pathlib import Path

#: The only sender-capture tool macOS provides; absence is recorded, not fatal.
ESLOGGER = '/usr/bin/eslogger'
#: DiagnosticReports directories that hold the code-signing/jetsam verdicts a
#: host kill leaves behind. Either may be absent and is recorded, never fatal.
IPS_DIRECTORIES = (
    ('system', '/Library/Logs/DiagnosticReports'),
    ('user', '~/Library/Logs/DiagnosticReports'),
)
#: Seconds of slack around the command window for a report written after exit.
IPS_WINDOW_SLACK = 120
#: The `log show` invocation for the kernel and victim-pid window.
KERNEL_LOG_COMMAND = ('log show --info --debug --start {start} --end {end} '
                      '--predicate {predicate}')
#: A full-window `log show` can be slow; the profiler bounds it and records a miss.
KERNEL_LOG_TIMEOUT = 120
#: `log show` reads the host's local clock, as `datetime.now()` does.
LOG_WINDOW_FORMAT = '%Y-%m-%d %H:%M:%S'
#: The server logs this warning line when it reaps a child killed by SIGKILL.
REAP_PID = re.compile(r'child reap pid=(\d+)\b[^\n]*\bsignal=9\b')
#: Host samplers: process table plus two independent memory sources. A host
#: without one (Linux has neither `memory_pressure` nor `vm_stat`) records it as
#: unavailable and keeps going.
SAMPLER_COMMANDS = (
    ('ps', ('ps', '-axo', 'pid,ppid,pgid,rss,vsz,etime,comm')),
    ('memory_pressure', ('memory_pressure', '-Q')),
    ('vm_stat', ('vm_stat',)),
    ('swap', ('sysctl', 'vm.swapusage')),
)
#: Executable names whose process counts as part of the test/server/Git tree
#: even when it has been reparented away from the wrapped command.
WATCHED_COMMS = (
    'magit-lsp', 'git', 'cargo', 'rustc', 'cc', 'ld',
    'server_async', 'editor_bridge', 'rebase_interactive',
)


def record(log, message):
    """Append one diagnostic line to ``log``; never raise into the caller."""
    try:
        log.write(message.rstrip('\n') + '\n')
        log.flush()
    except OSError:
        pass


def parse_args(argv):
    """Split ``argv`` at ``--`` into the options and the wrapped command."""
    if '--' not in argv:
        raise SystemExit('profile_ci_test: pass the wrapped command after --')
    split = argv.index('--')
    parser = argparse.ArgumentParser(
        add_help=True,
        description='Profile a wrapped CI test command (macOS lane evidence).')
    parser.add_argument('--output-dir', required=True,
                        help='directory for samples.log, summary.json and logs')
    parser.add_argument('--interval', type=float, default=1.0,
                        help='seconds between samples (default 1)')
    options = parser.parse_args(argv[:split])
    command = argv[split + 1:]
    if not command:
        raise SystemExit('profile_ci_test: empty command after --')
    return options, command


def exit_status(returncode):
    """Map a negative signal code to the shell's 128+signal convention."""
    if returncode is None:
        return 1
    return returncode if returncode >= 0 else 128 - returncode


def parse_ps(text):
    """Return sampled process rows from ``ps -axo`` output, ignoring junk lines."""
    rows = []
    for line in text.splitlines()[1:]:
        fields = line.split(None, 6)
        if len(fields) < 7:
            continue
        pid, ppid, pgid, rss, vsz, etime, comm = fields
        try:
            rows.append({'pid': int(pid), 'ppid': int(ppid), 'pgid': int(pgid),
                         'rss_kib': int(rss), 'vsz_kib': int(vsz),
                         'etime': etime, 'comm': comm.strip()})
        except ValueError:
            continue
    return rows


def tree_rows(rows, root_pid):
    """Return the sampled rows inside the wrapped command's process tree.

    A row belongs when it descends from ``root_pid`` or its executable name is
    one of the watched tools. The set is recomputed per sample because test and
    Git children are short-lived and may be reparented when their parent exits.
    """
    children = {}
    for row in rows:
        children.setdefault(row['ppid'], []).append(row['pid'])
    descendants = set()
    pending = [root_pid]
    while pending:
        pid = pending.pop()
        for child in children.get(pid, []):
            if child not in descendants:
                descendants.add(child)
                pending.append(child)
    watched = set(descendants)
    for row in rows:
        base = os.path.basename(row['comm'])
        if any(base == name or base.startswith(name) for name in WATCHED_COMMS):
            watched.add(row['pid'])
    return [row for row in rows if row['pid'] in watched]


def sample_processes(handle, index, state, root_pid):
    """Run one sampler pass, write it to ``handle`` and fold in the peaks.

    A sampler that is absent or fails is recorded in ``state['samplers']`` and
    skipped, so a host without ``memory_pressure`` still profiles.
    """
    stamp = datetime.now(timezone.utc).isoformat(timespec='milliseconds')
    sections = [f'--- sample {index} {stamp} ---']
    rows = []
    for name, argv in SAMPLER_COMMANDS:
        try:
            result = subprocess.run(argv, capture_output=True, text=True,
                                    timeout=15)
        except (OSError, subprocess.SubprocessError) as error:
            state['samplers'][name] = 'unavailable'
            sections.append(f'{name}: unavailable ({error.__class__.__name__})')
            continue
        if result.returncode != 0:
            state['samplers'][name] = f'exit {result.returncode}'
            sections.append(f'{name}: exit {result.returncode}')
            continue
        state['samplers'][name] = 'ok'
        sections.append(f'$ {" ".join(argv)}')
        sections.append(result.stdout.rstrip('\n'))
        if result.stderr.strip():
            sections.append('stderr: ' + result.stderr.strip())
        if name == 'ps':
            rows = parse_ps(result.stdout)
    handle.write('\n'.join(sections) + '\n')
    handle.flush()
    if rows:
        largest = max(rows, key=lambda row: row['rss_kib'])
        if largest['rss_kib'] > state['peak_process_rss']['rss_kib']:
            state['peak_process_rss'] = {
                'pid': largest['pid'], 'comm': largest['comm'],
                'rss_kib': largest['rss_kib']}
    tree = tree_rows(rows, root_pid)
    state['peak_tree_rss_kib'] = max(
        state['peak_tree_rss_kib'], sum(row['rss_kib'] for row in tree))
    state['sample_count'] = index


def start_sender_capture(directory, log):
    """Start ``sudo eslogger signal`` and return its state, recording refusal.

    ``sudo -n`` never prompts, so a password requirement or a policy denial
    returns immediately and is recorded as ``denied``; the run continues without
    sender capture. Absence of ``/usr/bin/eslogger`` is recorded as ``absent``.
    """
    state: dict = {'status': 'absent', 'path': ESLOGGER, 'detail': 'not present'}
    if not os.path.exists(ESLOGGER):
        record(log, f'eslogger: absent, {ESLOGGER} is not present on this host')
        return state
    try:
        stdout = open(directory / 'signal-capture.log', 'wb')
        stderr = open(directory / 'signal-capture.err', 'wb')
    except OSError as error:
        record(log, f'eslogger: cannot open capture files ({error})')
        state['detail'] = f'capture files: {error}'
        return state
    try:
        process = subprocess.Popen(['sudo', '-n', ESLOGGER, 'signal'],
                                   stdout=stdout, stderr=stderr)
    except OSError as error:
        stdout.close()
        stderr.close()
        record(log, f'eslogger: could not start ({error})')
        state['status'] = 'denied'
        state['detail'] = str(error)
        return state
    record(log, f'eslogger: started pid={process.pid} {ESLOGGER} signal')
    state['status'] = 'running'
    state['detail'] = ''
    state['process'] = process
    state['streams'] = (stdout, stderr)
    return state


def stop_sender_capture(state, directory, log):
    """Terminate ``eslogger`` and resolve its status from its exit code."""
    process = state.get('process')
    streams = state.get('streams', ())
    if process is None:
        for stream in streams:
            stream.close()
        return
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        state['status'] = 'ok'
    elif process.returncode == 0:
        state['status'] = 'ok'
    else:
        detail = ''
        try:
            detail = (directory / 'signal-capture.err').read_text(
                encoding='utf-8', errors='replace').strip().splitlines()
            detail = detail[-1] if detail else ''
        except OSError:
            pass
        state['status'] = 'denied'
        state['detail'] = detail or f'eslogger exited {process.returncode}'
    for stream in streams:
        stream.close()
    record(log, f'eslogger: status={state["status"]} {state["detail"]}')
    state.pop('process', None)
    state.pop('streams', None)


def parse_victim_pids(text):
    """Return the pids the server reaped after a ``SIGKILL``, in first-seen order.

    The server logs ``child reap pid=N ... signal=9`` at warn level when a child
    dies by signal, so the wrapped command's own output is the only source and
    no macOS host is needed to parse it.
    """
    pids = []
    for match in REAP_PID.finditer(text):
        pid = int(match.group(1))
        if pid not in pids:
            pids.append(pid)
    return pids


def log_window(started, finished):
    """Format local-clock bounds for ``log show --start``/``--end``.

    Both values come from this process's ``datetime.now()``, the same host clock
    ``log`` reads, so the formatting is pure and testable off a macOS host.
    """
    if finished < started:
        finished = started
    return started.strftime(LOG_WINDOW_FORMAT), finished.strftime(LOG_WINDOW_FORMAT)


def kernel_predicate(victims):
    """Return the ``log show`` predicate: kernel senders plus any victim pid.

    With no parsed victim pid the filter stays kernel-only, which still carries
    the code-signing and jetsam verdicts an external kill leaves behind.
    """
    clauses = ['sender == "kernel"']
    clauses.extend(f'eventMessage CONTAINS[c] "{pid}"' for pid in victims)
    return ' OR '.join(clauses)


def collect_ips_reports(directory, victims, started, finished, log,
                        platform=None):
    """Copy the DiagnosticReports files written during the command window.

    Returns the collector's status dict for ``summary.json``. The directories
    are Darwin-only, so another host records ``unavailable``; an absent
    directory is ``absent`` and an unreadable one ``denied``. Reports are
    selected by mtime inside the window because a shared runner keeps unrelated
    history in the same directories.
    """
    platform = sys.platform if platform is None else platform
    status = {'status': 'unavailable', 'directories': [], 'window': None,
              'copied': [], 'victim_pids': sorted(victims), 'detail': ''}
    if platform != 'darwin':
        status['detail'] = f'Darwin only (sys.platform={platform!r})'
        record(log, f'diagnostic reports: unavailable, {status["detail"]}')
        return status
    start, end = log_window(started, finished)
    status['window'] = [start, end]
    lower = started.timestamp() - IPS_WINDOW_SLACK
    upper = finished.timestamp() + IPS_WINDOW_SLACK
    readable = denied = False
    for label, pattern in IPS_DIRECTORIES:
        root = Path(pattern).expanduser()
        entry = {'path': str(root), 'status': 'absent', 'reports': 0,
                 'copied': 0}
        status['directories'].append(entry)
        if not root.is_dir():
            record(log, f'diagnostic reports: {root} is absent')
            continue
        try:
            reports = sorted(root.glob('*.ips'))
        except OSError as error:
            entry['status'] = 'denied'
            denied = True
            record(log, f'diagnostic reports: cannot list {root} ({error})')
            continue
        readable = True
        entry['status'] = 'ok'
        entry['reports'] = len(reports)
        for report in reports:
            try:
                inside = lower <= report.stat().st_mtime <= upper
            except OSError:
                continue
            if not inside:
                continue
            target = directory / 'diagnostic-reports' / f'{label}-{report.name}'
            try:
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(report, target)
            except OSError as error:
                denied = True
                record(log, f'diagnostic reports: cannot copy {report} ({error})')
                continue
            entry['copied'] += 1
            status['copied'].append(target.name)
    if readable:
        status['status'] = 'ok'
    elif denied:
        status['status'] = 'denied'
    else:
        status['status'] = 'absent'
        status['detail'] = 'no DiagnosticReports directory on this host'
    record(log, f'diagnostic reports: status={status["status"]} '
                f'copied={len(status["copied"])}')
    return status


def collect_kernel_log(directory, victims, started, finished, log,
                       platform=None):
    """Run ``log show`` for kernel messages and victim pids over the window.

    Returns the collector's status dict. A missing ``log``, a timeout or a
    non-zero exit is recorded and never raised, so an unprivileged host still
    profiles. Output lands in ``kernel-log.log`` inside ``--output-dir``.
    """
    platform = sys.platform if platform is None else platform
    status = {'status': 'unavailable', 'command': None, 'window': None,
              'path': None, 'victim_pids': sorted(victims), 'detail': ''}
    if platform != 'darwin':
        status['detail'] = f'Darwin only (sys.platform={platform!r})'
        record(log, f'kernel log: unavailable, {status["detail"]}')
        return status
    start, end = log_window(started, finished)
    predicate = kernel_predicate(victims)
    command = KERNEL_LOG_COMMAND.format(start=start, end=end, predicate=predicate)
    status['window'] = [start, end]
    status['command'] = command
    argv = ['log', 'show', '--info', '--debug', '--start', start, '--end', end,
            '--predicate', predicate]
    try:
        result = subprocess.run(argv, capture_output=True, text=True,
                                timeout=KERNEL_LOG_TIMEOUT)
    except (OSError, subprocess.SubprocessError) as error:
        status['detail'] = f'{error.__class__.__name__}: {error}'
        record(log, f'kernel log: unavailable ({status["detail"]})')
        return status
    if result.returncode != 0:
        lines = result.stderr.strip().splitlines()
        status['status'] = f'exit {result.returncode}'
        status['detail'] = lines[-1] if lines else ''
        record(log, f'kernel log: {status["status"]} {status["detail"]}')
        return status
    path = directory / 'kernel-log.log'
    try:
        path.write_text(result.stdout, encoding='utf-8', errors='replace')
    except OSError as error:
        status['status'] = 'denied'
        status['detail'] = str(error)
        record(log, f'kernel log: cannot write {path} ({error})')
        return status
    status['status'] = 'ok'
    status['path'] = path.name
    status['lines'] = len(result.stdout.splitlines())
    record(log, f'kernel log: status=ok lines={status["lines"]} {path.name}')
    return status


def collect_host_verdicts(directory, state, started, finished, capture_path,
                          log):
    """Run the post-exit collectors and record each status in ``state``.

    Both collectors are best effort: a non-Darwin host records ``unavailable``
    and a macOS host records the absence or denial of its sources, while the
    profiler keeps returning the wrapped command's status unchanged.
    """
    victims = []
    try:
        victims = parse_victim_pids(
            capture_path.read_text(encoding='utf-8', errors='replace'))
    except OSError as error:
        record(log, f'host verdicts: cannot read {capture_path} ({error})')
    record(log, f'host verdicts: victim pids {victims!r}')
    state['ips_reports'] = collect_ips_reports(
        directory, victims, started, finished, log)
    state['kernel_log'] = collect_kernel_log(
        directory, victims, started, finished, log)


def _drain_pipe(source, target_fd, capture, lock):
    """Copy one child pipe to the inherited descriptor and the capture file."""
    try:
        while True:
            chunk = source.read(65536)
            if not chunk:
                return
            if capture is not None:
                with lock:
                    try:
                        capture.write(chunk)
                        capture.flush()
                    except (OSError, ValueError):
                        pass
            offset = 0
            while offset < len(chunk):
                try:
                    offset += os.write(target_fd, chunk[offset:])
                except OSError:
                    return
    finally:
        try:
            source.close()
        except OSError:
            pass


def start_child_tee(command, directory):
    """Start ``command`` with its streams teed to the caller and a capture file.

    ``Popen(command)`` alone hands the child the caller's descriptors, and
    ``capture_output`` would buffer both streams and change the test's timing. A
    drain thread per pipe forwards every chunk to the inherited descriptor as it
    arrives, so the caller's bytes and timing are unchanged, and appends the same
    bytes to ``child-output.log`` for post-exit reap-line parsing.
    """
    try:
        capture = open(directory / 'child-output.log', 'wb')
    except OSError:
        capture = None
    lock = threading.Lock()
    try:
        child = subprocess.Popen(command, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE)
    except OSError:
        if capture is not None:
            capture.close()
        raise
    threads = []
    for pipe, default_fd, stream in ((child.stdout, 1, sys.stdout),
                                     (child.stderr, 2, sys.stderr)):
        try:
            fd = stream.fileno()
        except (AttributeError, OSError, ValueError):
            fd = default_fd
        thread = threading.Thread(target=_drain_pipe,
                                  args=(pipe, fd, capture, lock), daemon=True)
        thread.start()
        threads.append(thread)
    return {'process': child, 'threads': threads, 'capture': capture,
            'capture_path': directory / 'child-output.log'}


def finish_child_tee(tee, log):
    """Join the drain threads and close the capture file; never raise.

    A grandchild (the nix shell) can outlive the wrapped command and keep the
    pipes open, so the join is bounded and a still-draining thread is recorded.
    """
    for thread in tee['threads']:
        thread.join(timeout=5)
        if thread.is_alive():
            record(log, 'child streams: a drain thread is still running at exit')
    capture = tee.get('capture')
    if capture is not None:
        try:
            capture.close()
        except OSError:
            pass
        tee['capture'] = None


def write_summary(directory, state, log):
    """Write ``summary.json``; a write failure is recorded and never raised."""
    try:
        (directory / 'summary.json').write_text(
            json.dumps(state, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    except OSError as error:
        record(log, f'summary: could not write summary.json ({error})')


def profile_child(child, directory, interval, state, log):
    """Sample until ``child`` exits and return the status to propagate."""
    with open(directory / 'samples.log', 'w', encoding='utf-8') as samples:
        index = 0
        while True:
            index += 1
            sample_processes(samples, index, state, child.pid)
            if child.poll() is not None:
                return exit_status(child.returncode)
            time.sleep(max(interval, 0.1))


def main(argv=None):
    """Run the wrapped command under the profiler and return its exit code."""
    options, command = parse_args(sys.argv[1:] if argv is None else argv)
    directory = Path(options.output_dir)
    directory.mkdir(parents=True, exist_ok=True)
    state = {
        'command': command,
        'output_dir': str(directory.resolve()),
        'interval_seconds': options.interval,
        'started_at': datetime.now(timezone.utc).isoformat(timespec='milliseconds'),
        'finished_at': None,
        'sample_count': 0,
        'exit_code': None,
        'peak_process_rss': {'pid': None, 'comm': None, 'rss_kib': 0},
        'peak_tree_rss_kib': 0,
        'samplers': {},
        'eslogger': {'status': 'not-probed', 'path': ESLOGGER, 'detail': ''},
        'ips_reports': {'status': 'not-run'},
        'kernel_log': {'status': 'not-run'},
    }
    exit_code = 1
    child = None
    tee = None
    started = datetime.now()
    with open(directory / 'profiler.log', 'a', encoding='utf-8') as log:
        record(log, f'profiler: command {command!r}')
        capture = start_sender_capture(directory, log)
        try:
            tee = start_child_tee(command, directory)
            child = tee['process']
            exit_code = profile_child(child, directory, options.interval, state, log)
        except Exception as error:  # never mask the wrapped command's status
            record(log, f'profiler: internal failure: {error!r}')
            traceback.print_exc(file=log)
            if child is not None and child.poll() is None:
                exit_code = exit_status(child.wait())
        finally:
            finished = datetime.now()
            if tee is not None:
                finish_child_tee(tee, log)
            try:
                stop_sender_capture(capture, directory, log)
            except Exception as error:  # cleanup must never mask the exit status
                record(log, f'profiler: sender capture cleanup failed: {error!r}')
            try:
                if tee is not None and child is not None \
                        and child.poll() is not None:
                    collect_host_verdicts(directory, state, started, finished,
                                          tee['capture_path'], log)
            except Exception as error:  # host evidence must never mask the status
                record(log, f'profiler: host verdict collection failed: {error!r}')
            state['finished_at'] = datetime.now(
                timezone.utc).isoformat(timespec='milliseconds')
            state['exit_code'] = exit_code
            state['eslogger'] = capture
            write_summary(directory, state, log)
    return exit_code


if __name__ == '__main__':
    sys.exit(main())
