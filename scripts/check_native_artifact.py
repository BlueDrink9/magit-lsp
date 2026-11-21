#!/usr/bin/env python3
"""Inspect a release archive's static native runtime requirements without execution."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

TARGETS = {
    'x86_64-unknown-linux-gnu': ('linux', 'x86_64', 'readelf'),
    'aarch64-apple-darwin': ('darwin', 'arm64', 'otool'),
    'x86_64-apple-darwin': ('darwin', 'x86_64', 'otool'),
}
MAX_GLIBC = (2, 35)


class InspectionError(Exception):
    """An artifact or inspection tool failed a static contract."""


def digest(path):
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


def inspect(artifact):
    """Verify identity and static ABI of one archive; never execute its binary."""
    archive = Path(artifact).resolve()
    result = {'archive': str(archive), 'archive_sha256': None, 'target': None,
              'executable_sha256': None, 'inspection': {}, 'teardown': 'complete', 'ok': False}
    try:
        if not archive.is_absolute() or not archive.is_file():
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
        result['ok'] = True
    except Exception as error:
        result['error'] = str(error)
    return result


def main(argv):
    if len(argv) != 1:
        print('usage: make native-artifact-inspect RELEASE_ARTIFACT=/absolute/path/to/archive', file=sys.stderr)
        return 2
    result = inspect(argv[0])
    print(json.dumps(result, sort_keys=True))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main(sys.argv[1:]))
