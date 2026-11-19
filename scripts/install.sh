#!/bin/sh
# Reviewed download-only installer; requires Python 3 and curl, never sudo.
set -eu
command -v python3 >/dev/null 2>&1 || { echo 'install: Python 3 is required' >&2; exit 1; }
exec python3 - "$@" <<'PY'
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile


def fail(message):
    raise ValueError(message)


def version(value):
    if not isinstance(value, str):
        fail('release tag must be a string')
    value = value.removeprefix('v')
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', value):
        fail('version must be a stable X.Y.Z or vX.Y.Z')
    return value


def download(url, destination):
    subprocess.run([
        'curl', '--fail', '--silent', '--show-error', '--location',
        '--proto', '=https', '--proto-redir', '=https', '--tlsv1.2',
        '--connect-timeout', '15', '--max-time', '180',
        '--output', str(destination), url,
    ], check=True)


def main():
    parser = argparse.ArgumentParser(description='Install a GitHub Release binary without Rust or sudo')
    parser.add_argument('--version', default='latest', help='latest or stable X.Y.Z (optional v prefix)')
    parser.add_argument('--install-dir', default=str(Path.home() / '.local/bin'))
    args = parser.parse_args()
    system = subprocess.check_output(['uname', '-s'], text=True).strip()
    machine = subprocess.check_output(['uname', '-m'], text=True).strip()
    if (system, machine) != ('Linux', 'x86_64'):
        fail('unsupported platform; only Linux x86_64 GNU is available')
    # musl Linux also reports Linux/x86_64, but cannot run this GNU artifact.
    libc = subprocess.run(['getconf', 'GNU_LIBC_VERSION'], capture_output=True, text=True)
    if libc.returncode or not libc.stdout.startswith('glibc '):
        fail('unsupported libc; GNU glibc 2.35 or newer is required')
    numbers = tuple(int(n) for n in libc.stdout.split()[1].split('.'))
    if numbers < (2, 35):
        fail('GNU glibc 2.35 or newer is required')
    with tempfile.TemporaryDirectory(prefix='magit-lsp-download-') as temporary:
        temporary = Path(temporary)
        selected = args.version
        if selected == 'latest':
            metadata = temporary / 'latest.json'
            download('https://api.github.com/repos/BlueDrink9/magit-lsp/releases/latest', metadata)
            selected = json.loads(metadata.read_text())['tag_name']
        selected = version(selected)
        stem = f'magit-lsp-v{selected}-x86_64-unknown-linux-gnu'
        archive_name = stem + '.tar.gz'
        base = f'https://github.com/BlueDrink9/magit-lsp/releases/download/v{selected}'
        archive = temporary / archive_name
        checksum = temporary / 'checksum'
        download(f'{base}/{archive_name}.sha256', checksum)
        download(f'{base}/{archive_name}', archive)
        manifest = checksum.read_text()
        if not re.fullmatch(r'[0-9a-f]{64}  ' + re.escape(archive_name) + r'\n', manifest):
            fail('invalid checksum manifest')
        if archive.stat().st_size > 128 * 1024 * 1024:
            fail('archive exceeds size limit')
        if hashlib.sha256(archive.read_bytes()).hexdigest() != manifest[:64]:
            fail('checksum mismatch')
        expected = {f'{stem}/{name}' for name in ('magit-lsp', 'LICENSE', 'LICENSE-APACHE', 'INSTALL.md')}
        with tarfile.open(archive, 'r:gz') as package:
            seen = set()
            binary = None
            for member in package:
                if member.name not in expected or member.name in seen or not member.isfile():
                    fail('unexpected archive structure (paths, links or duplicates)')
                seen.add(member.name)
                limit = 256 * 1024 * 1024 if member.name.endswith('/magit-lsp') else 1024 * 1024
                if not 0 < member.size <= limit:
                    fail('invalid archive member size')
                if member.name.endswith('/magit-lsp'):
                    binary = package.extractfile(member).read()
            if seen != expected:
                fail('archive is missing required files')
        # Never extract paths from the archive. Stage only the validated binary,
        # on the destination filesystem, so replacement is one atomic rename.
        destination = Path(args.install_dir).expanduser().resolve()
        destination.mkdir(parents=True, exist_ok=True)
        target = destination / 'magit-lsp'
        staging = None
        try:
            with tempfile.NamedTemporaryFile(prefix='.magit-lsp-', dir=destination, delete=False) as output:
                staging = Path(output.name)
                output.write(binary)
                output.flush()
                os.fsync(output.fileno())
            staging.chmod(0o755)
            os.replace(staging, target)
        finally:
            if staging is not None:
                staging.unlink(missing_ok=True)
        print(f'Installed magit-lsp {selected} to {target}; add {destination} to PATH if needed.')


try:
    main()
except (OSError, EOFError, ValueError, KeyError, TypeError, IndexError, tarfile.TarError, subprocess.SubprocessError) as error:
    print(f'install: {error}', file=sys.stderr)
    sys.exit(1)
PY
