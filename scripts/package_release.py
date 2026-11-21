#!/usr/bin/env python3
"""Build a deterministic release archive for one validated target artifact.

The shell wrapper ``scripts/package-release.sh`` is the only entry point. This
module reads an already-built binary and an explicit trusted provenance record,
never executes the binary, and writes byte-identical archive and sidecar bytes
for identical inputs regardless of their paths or modification times.
"""
from __future__ import annotations

import gzip
import hashlib
import io
import json
import re
from pathlib import Path
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[1]
VERSION_PATTERN = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)')
HASH_PATTERN = re.compile(r'[0-9a-f]{64}')
# Archive metadata is fixed so bytes depend only on member content and names,
# never on wall-clock time, locale or the packing host.
FIXED_MTIME = 0
EXECUTABLE_MODE = 0o755
DATA_MODE = 0o644
# Intended release targets. Windows has no build or gate yet, so it is absent;
# adding it later must name its executable explicitly here.
TARGETS = {
    'x86_64-unknown-linux-gnu': {
        'executable': 'magit-lsp',
        'install_notes': 'Linux x86_64 GNU only; glibc >= 2.35 and Git required.',
    },
    'aarch64-apple-darwin': {
        'executable': 'magit-lsp',
        'install_notes': 'macOS arm64; Git required.',
    },
    'x86_64-apple-darwin': {
        'executable': 'magit-lsp',
        'install_notes': 'macOS x86_64; Git required.',
    },
}
LICENCES = (
    ('LICENCES/GPL-3.0', 'LICENSE'),
    ('LICENCES/Apache-2.0', 'LICENSE-APACHE'),
)


class PackagingError(Exception):
    """One failed packaging step, reported without a traceback."""


def stable_version(value):
    """Return the normalized stable X.Y.Z version or refuse the input."""
    if not isinstance(value, str):
        raise PackagingError('version must be a string')
    normalized = value.removeprefix('v')
    if not VERSION_PATTERN.fullmatch(normalized):
        raise PackagingError('version must be stable X.Y.Z or vX.Y.Z')
    return normalized


def manifest_versions():
    """Return the Cargo and extension versions that must match the request."""
    match = re.search(r'^version = "([^"]+)"', (ROOT / 'Cargo.toml').read_text(), re.M)
    if match is None:
        raise PackagingError('Cargo.toml is missing its version')
    try:
        extension = json.loads((ROOT / 'clients/vscode/package.json').read_text())['version']
    except (OSError, KeyError, TypeError, ValueError) as error:
        raise PackagingError(f'cannot read the extension version: {error}') from error
    return match[1], extension


def load_provenance(path, version, target, executable_sha256):
    """Validate explicit trusted provenance against the requested identity.

    The provenance record binds the version, target and executable digest that
    the trusted build or run manifest produced. Packaging refuses a missing,
    malformed or contradictory record instead of asking the binary itself.
    """
    try:
        data = json.loads(Path(path).read_text(encoding='utf-8'))
    except OSError as error:
        raise PackagingError(f'cannot read provenance {path}: {error.strerror or error}') from error
    except json.JSONDecodeError as error:
        raise PackagingError(f'provenance {path} is not valid JSON') from error
    if not isinstance(data, dict):
        raise PackagingError('provenance must be a JSON object')
    for key in ('version', 'target', 'executable_sha256'):
        if key not in data:
            raise PackagingError(f'provenance is missing {key}')
        if not isinstance(data[key], str):
            raise PackagingError(f'provenance {key} must be a string')
    if data['version'] != version:
        raise PackagingError('provenance version does not match the requested version')
    if data['target'] != target:
        raise PackagingError('provenance target does not match the requested target')
    if not HASH_PATTERN.fullmatch(data['executable_sha256']):
        raise PackagingError('provenance executable_sha256 must be 64 lowercase hex digits')
    if data['executable_sha256'] != executable_sha256:
        raise PackagingError('binary does not match the provenance executable_sha256')


def installation_notes(version, target):
    """Return the target-specific INSTALL.md bytes."""
    return (
        f'magit-lsp {version}\n\n'
        f'{TARGETS[target]["install_notes"]}\n'
        'Put magit-lsp on PATH (for example ~/.local/bin); no Rust is needed.\n'
        'See docs/RELEASING.md in the source for baseline and verification status.\n'
        'LICENSE is GPL-3.0-only; LICENSE-APACHE covers Apache-2.0-licensed protocol and client components.\n'
    ).encode('utf-8')


def archive_members(binary, version, target):
    """Return the ordered ``(name, content, mode)`` archive members."""
    executable = TARGETS[target]['executable']
    members = [(executable, binary, EXECUTABLE_MODE)]
    members.extend((name, (ROOT / source).read_bytes(), DATA_MODE) for source, name in LICENCES)
    members.append(('INSTALL.md', installation_notes(version, target), DATA_MODE))
    return members


def write_archive(archive, stem, members):
    """Write a normalized tar.gz whose bytes ignore host paths and times."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode='w', format=tarfile.PAX_FORMAT) as package:
        for name, content, mode in members:
            info = tarfile.TarInfo(f'{stem}/{name}')
            info.type = tarfile.REGTYPE
            info.mode = mode
            info.size = len(content)
            info.mtime = FIXED_MTIME
            info.uid = info.gid = 0
            info.uname = info.gname = ''
            package.addfile(info, io.BytesIO(content))
    with archive.open('wb') as raw:
        with gzip.GzipFile(filename='', mode='wb', fileobj=raw,
                           mtime=FIXED_MTIME, compresslevel=9) as compressed:
            compressed.write(buffer.getvalue())


def package(version_argument, binary, output, target, provenance):
    """Write the deterministic archive and SHA-256 sidecar for one artifact.

    ``target`` must be an intended release target and ``provenance`` a JSON
    record with matching ``version``, ``target`` and ``executable_sha256``.
    The binary is read as bytes only and is never executed.
    """
    version = stable_version(version_argument)
    cargo, extension = manifest_versions()
    if version != cargo or version != extension:
        raise PackagingError('Cargo, extension and requested versions must match')
    if target not in TARGETS:
        raise PackagingError(
            f'unsupported target {target!r}; expected one of ' + ', '.join(sorted(TARGETS)))
    binary = Path(binary)
    try:
        content = binary.read_bytes()
    except OSError as error:
        raise PackagingError(f'cannot read binary {binary}: {error.strerror or error}') from error
    load_provenance(provenance, version, target, hashlib.sha256(content).hexdigest())
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    stem = f'magit-lsp-v{version}-{target}'
    archive = output / (stem + '.tar.gz')
    write_archive(archive, stem, archive_members(content, version, target))
    checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
    Path(str(archive) + '.sha256').write_text(f'{checksum}  {archive.name}\n', encoding='utf-8')
    print(archive)


def main(arguments):
    """Run the packager with the wrapper's positional arguments."""
    if len(arguments) != 5:
        print('usage: sh scripts/package-release.sh VERSION BINARY OUTPUT_DIR TARGET PROVENANCE',
              file=sys.stderr)
        return 2
    try:
        package(*arguments)
    except (OSError, PackagingError, ValueError, KeyError, TypeError, tarfile.TarError) as error:
        print(f'package-release: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main(sys.argv[1:]))
