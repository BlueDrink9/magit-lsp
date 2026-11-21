#!/usr/bin/env python3
"""Inspect a release archive's static native runtime requirements without execution."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import select
import subprocess
import sys
import tarfile
import tempfile
import platform
import time

TARGETS = {
    'x86_64-unknown-linux-gnu': ('linux', 'x86_64', 'readelf'),
    'aarch64-apple-darwin': ('darwin', 'arm64', 'otool'),
    'x86_64-apple-darwin': ('darwin', 'x86_64', 'otool'),
}
MAX_GLIBC = (2, 35)


class InspectionError(Exception):
    """An artifact or inspection tool failed a static contract."""


def _rpc(process, method, params, request_id):
    """Send one bounded framed JSON-RPC request and return its matching result."""
    payload = json.dumps({'jsonrpc': '2.0', 'id': request_id, 'method': method,
                          'params': params}).encode()
    process.stdin.write(f'Content-Length: {len(payload)}\r\n\r\n'.encode() + payload)
    process.stdin.flush()
    deadline = time.monotonic() + 15
    while True:
        buffer = getattr(process, '_native_rpc_buffer', b'')
        while b'\r\n\r\n' not in buffer:
            ready, _, _ = select.select([process.stdout], [], [], max(0, deadline - time.monotonic()))
            if not ready:
                raise InspectionError(f'timed out waiting for {method}')
            chunk = os.read(process.stdout.fileno(), 4096)
            if not chunk:
                raise InspectionError('server closed JSON-RPC stream')
            buffer += chunk
        header, body = buffer.split(b'\r\n\r\n', 1)
        lengths = [line.split(b':', 1)[1].strip() for line in header.split(b'\r\n')
                   if line.lower().startswith(b'content-length:')]
        if len(lengths) != 1:
            raise InspectionError('invalid JSON-RPC headers')
        length = int(lengths[0])
        if length <= 0 or length > 4 * 1024 * 1024:
            raise InspectionError('invalid JSON-RPC frame length')
        while len(body) < length:
            ready, _, _ = select.select([process.stdout], [], [], max(0, deadline - time.monotonic()))
            if not ready:
                raise InspectionError(f'timed out reading {method} frame')
            body += os.read(process.stdout.fileno(), min(4096, length - len(body)))
        message = json.loads(body[:length])
        process._native_rpc_buffer = body[length:]
        if message.get('method') == 'view/render':
            params = message.get('params', {})
            if params.get('viewId'):
                _notify(process, 'view/rendered', {'viewId': params['viewId'],
                                                   'version': params['version']})
                process._native_render_count = getattr(process, '_native_render_count', 0) + 1
                process._native_render_version = params['version']
            continue
        if 'method' in message and 'id' not in message:
            continue
        if message.get('id') != request_id:
            raise InspectionError(f'unexpected JSON-RPC message: {message}')
        if 'error' in message:
            raise InspectionError(f'JSON-RPC {method} failed: {message["error"]}')
        return message.get('result')


def _notify(process, method, params):
    """Send one JSON-RPC notification without waiting for a response."""
    payload = json.dumps({'jsonrpc': '2.0', 'method': method, 'params': params}).encode()
    process.stdin.write(f'Content-Length: {len(payload)}\r\n\r\n'.encode() + payload)
    process.stdin.flush()


def _stop_process(process, evidence):
    """Attempt protocol shutdown, then terminate, kill and reap the owned child."""
    try:
        _rpc(process, 'shutdown', None, 4)
        _notify(process, 'exit', None)
    except Exception as error:
        evidence['shutdown_error'] = str(error)
    if process.poll() is None:
        process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
    evidence['children_remaining'] = process.poll() is None
    for stream in (getattr(process, 'stdin', None), getattr(process, 'stdout', None)):
        if stream:
            stream.close()


def _functional(binary, target, version_expected, evidence):
    """Run version, LSP status and an index-verified mutation on the exact binary."""
    expected = TARGETS[target]
    if platform.system().lower() != expected[0] or platform.machine().lower() not in (
            {'x86_64', 'amd64'} if expected[1] == 'x86_64' else {'arm64', 'aarch64'}):
        raise InspectionError('host platform.machine/system does not match target')
    evidence['host'] = {'system': platform.system(), 'machine': platform.machine()}
    version = subprocess.run([str(binary), '--version'], check=True, capture_output=True,
                             text=True, timeout=15).stdout.strip()
    if version != f'magit-lsp {version_expected}':
        raise InspectionError(f'--version does not match manifest version {version_expected}: {version}')
    evidence['version'] = version
    with tempfile.TemporaryDirectory(prefix='native-consumer-') as repo:
        subprocess.run(['git', 'init', '-q', repo], check=True, timeout=15)
        subprocess.run(['git', '-C', repo, 'config', 'user.email', 'test@example.invalid'], check=True, timeout=15)
        subprocess.run(['git', '-C', repo, 'config', 'user.name', 'Native test'], check=True, timeout=15)
        file = Path(repo) / 'tracked.txt'
        file.write_text('before\n')
        subprocess.run(['git', '-C', repo, 'add', 'tracked.txt'], check=True, timeout=15)
        subprocess.run(['git', '-C', repo, 'commit', '-qm', 'initial'], check=True, timeout=15)
        file.write_text('after\n')
        process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL, cwd=repo, bufsize=0)
        evidence['teardown'] = 'incomplete'
        try:
            _rpc(process, 'initialize', {'processId': None, 'rootUri': Path(repo).as_uri(),
                 'capabilities': {'experimental': {'magit': {
                     'v': 1, 'keys': 'dynamic', 'modifiers': 'full',
                     'layout': ['main', 'side', 'bottom', 'float'],
                     'effects': ['visitFile', 'editDocument', 'clipboard'],
                     'prompts': ['confirm', 'text', 'choice', 'multiChoice', 'secret']}}}}, 1)
            _notify(process, 'initialized', {})
            opened = _rpc(process, 'view/open', {'kind': 'status', 'args': {'repo_path': repo},
                                'viewport': {'width': 80, 'height': 24}}, 2)
            view_id = opened.get('viewId') if isinstance(opened, dict) else None
            if not view_id:
                raise InspectionError('status view/open returned no viewId')
            _rpc(process, 'view/invoke', {'viewId': view_id, 'command': 'status.stage-all'}, 3)
            if getattr(process, '_native_render_count', 0) < 1:
                raise InspectionError('status view emitted no structured render to acknowledge')
            staged = subprocess.run(['git', '-C', repo, 'diff', '--cached', '--name-only'],
                                    check=True, capture_output=True, text=True, timeout=15).stdout.splitlines()
            if staged != ['tracked.txt']:
                raise InspectionError(f'Git index mutation mismatch: {staged}')
            evidence.update(lsp={'initialize': True, 'status_render_acks': process._native_render_count,
                                 'last_render_version': process._native_render_version},
                            git={'staged': staged})
            evidence['functional'] = {'outcome': 'success'}
        finally:
            _stop_process(process, evidence)
            evidence['teardown'] = 'complete' if not evidence.get('children_remaining') else 'failed'


def digest(path):
    """Return the SHA-256 digest of a file's unchanged bytes."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_tool(argv):
    """Run one inspection tool and return its text output."""
    try:
        result = subprocess.run(argv, check=True, text=True, capture_output=True)
    except (OSError, subprocess.CalledProcessError) as error:
        raise InspectionError(f'inspection tool failed: {argv[0]}: {error}') from error
    return result.stdout + result.stderr


def safe_extract(archive, destination, expected):
    """Extract exactly the regular files of the expected release archive."""
    try:
        package = tarfile.open(archive, 'r:gz')
        members = package.getmembers()
    except (OSError, tarfile.TarError) as error:
        raise InspectionError(f'invalid archive: {error}') from error
    for member in members:
        path = PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not member.isfile():
            raise InspectionError(f'unsafe archive member: {member.name}')
    names = {f'{expected}/{name}' for name in ('magit-lsp', 'LICENSE', 'LICENSE-APACHE', 'INSTALL.md')}
    if len(members) != len(names) or {m.name for m in members} != names:
        raise InspectionError('archive members do not match the exact expected set')
    package.extractall(destination, members=members, filter='data')
    return Path(destination) / expected / 'magit-lsp'


def inspect_elf(binary, target, evidence):
    header = run_tool(['readelf', '-h', str(binary)])
    if 'Advanced Micro Devices X86-64' not in header:
        raise InspectionError('ELF architecture does not match x86_64 target')
    program = run_tool(['readelf', '-l', str(binary)])
    dynamic = run_tool(['readelf', '-d', str(binary)])
    versions = run_tool(['readelf', '--version-info', str(binary)])
    combined = program + dynamic
    interpreter = re.search(r'Requesting program interpreter: \[?([^]\n]+)', program)
    if interpreter is None:
        raise InspectionError('ELF interpreter is missing')
    if '/nix/' in interpreter[1]:
        raise InspectionError('Nix ELF interpreter is forbidden')
    if re.search(r'(?:RPATH|RUNPATH).*\[.*(?:/nix/store|/nix/)', combined, re.I):
        raise InspectionError('Nix runtime path is forbidden')
    needed = re.findall(r'Shared library: \[([^]]+)\]', dynamic)
    if not needed or any('/nix/' in item for item in needed):
        raise InspectionError('ELF needed libraries are missing or use Nix')
    symbols = re.findall(r'GLIBC_(\d+(?:\.\d+)+)', versions)
    parsed = [tuple(map(int, value.split('.'))) for value in symbols]
    if not parsed or max(parsed) > MAX_GLIBC:
        raise InspectionError('GLIBC symbol requirement exceeds 2.35 or is missing')
    if re.search(r'GLIBCXX_|CXXABI_', versions):
        raise InspectionError('unsupported GLIBCXX/CXXABI version requirement')
    evidence.update(architecture='x86_64', interpreter=True, needed=needed,
                    glibc_versions=sorted(set(symbols)))


def inspect_macho(binary, target, evidence):
    arch = target[1]
    header = run_tool(['lipo', '-archs', str(binary)])
    expected = 'arm64' if arch == 'arm64' else 'x86_64'
    if expected not in header.split():
        raise InspectionError('Mach-O architecture does not match target')
    loads = run_tool(['otool', '-l', str(binary)])
    libs = run_tool(['otool', '-L', str(binary)])
    if '/nix/' in loads or '/nix/' in libs:
        raise InspectionError('Nix runtime path or library is forbidden')
    if not re.search(r'cmd LC_(?:BUILD_VERSION|VERSION_MIN_MACOSX)', loads):
        raise InspectionError('Mach-O deployment load command is missing')
    commands = re.findall(r'cmd LC_BUILD_VERSION.*?minos ([0-9.]+)|cmd LC_VERSION_MIN_MACOSX.*?version ([0-9.]+)', loads, re.S)
    versions = [a or b for a, b in commands]
    if len(versions) != 1 or not versions[0] or not re.fullmatch(r'\d+\.\d+(?:\.\d+)?', versions[0]):
        raise InspectionError('malformed or ambiguous Mach-O deployment version')
    dylibs = [line.strip().split(' (', 1)[0] for line in libs.splitlines()[1:] if line.strip()]
    if not dylibs:
        raise InspectionError('Mach-O linked libraries are missing')
    evidence.update(architecture=expected, dylibs=dylibs, deployment=versions[0],
                    rpaths=re.findall(r'path (\S+)', loads))


def inspect(artifact, functional=False):
    """Verify archive identity and static ABI, optionally probing the extracted binary."""
    supplied = Path(artifact)
    archive = supplied.resolve() if supplied.is_absolute() else supplied
    result = {'archive': str(archive), 'archive_sha256': None, 'target': None,
              'executable_sha256': None, 'inspection': {}, 'teardown': 'complete', 'ok': False}
    try:
        if not supplied.is_absolute() or not archive.is_file():
            raise InspectionError('candidate archive input is missing or not absolute')
        sidecar = Path(str(archive) + '.sha256')
        manifest_path = archive.parent / 'release-manifest.json'
        manifest_sum = archive.parent / 'release-manifest.json.sha256'
        if not sidecar.is_file() or not manifest_path.is_file() or not manifest_sum.is_file():
            raise InspectionError('required archive or manifest evidence is missing')
        result['archive_sha256'] = digest(archive)
        if sidecar.read_text() != f"{result['archive_sha256']}  {archive.name}\n":
            raise InspectionError('archive checksum sidecar mismatch')
        if manifest_sum.read_text() != f"{digest(manifest_path)}  release-manifest.json\n":
            raise InspectionError('manifest checksum mismatch')
        manifest = json.loads(manifest_path.read_text())
        target_name = manifest.get('target')
        if target_name not in TARGETS or manifest.get('archive_sha256') != result['archive_sha256']:
            raise InspectionError('manifest target or archive hash mismatch')
        result['target'] = target_name
        expected_stem = f"magit-lsp-v{manifest.get('version')}-{target_name}"
        if archive.name != expected_stem + '.tar.gz':
            raise InspectionError('archive name does not match manifest identity')
        with tempfile.TemporaryDirectory(prefix='native-artifact-') as temp:
            binary = safe_extract(archive, temp, expected_stem)
            result['executable_sha256'] = digest(binary)
            if result['executable_sha256'] != manifest.get('executable_sha256'):
                raise InspectionError('executable hash does not match manifest')
            if TARGETS[target_name][0] == 'linux':
                inspect_elf(binary, TARGETS[target_name], result['inspection'])
            else:
                inspect_macho(binary, TARGETS[target_name], result['inspection'])
            if functional:
                try:
                    _functional(binary, target_name, manifest.get('version'), result)
                finally:
                    result['executable_sha256_after'] = digest(binary)
                    if result['executable_sha256_after'] != result['executable_sha256']:
                        raise InspectionError('executable hash changed during acceptance')
            elif digest(binary) != result['executable_sha256']:
                raise InspectionError('executable hash changed during inspection')
        result['ok'] = True
    except Exception as error:
        result['error'] = str(error)
        if functional:
            result['functional'] = {'outcome': 'failed', 'teardown': result.get('teardown')}
    return result


def main(argv):
    if len(argv) not in (1, 2) or (len(argv) == 2 and argv[0] != '--functional'):
        print('usage: check_native_artifact.py [--functional] /absolute/path/to/archive', file=sys.stderr)
        return 2
    functional = argv[0] == '--functional'
    result = inspect(argv[1] if functional else argv[0], functional)
    print(json.dumps(result, sort_keys=True))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main(sys.argv[1:]))
