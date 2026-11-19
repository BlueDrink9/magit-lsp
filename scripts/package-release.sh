#!/bin/sh
# Package an already-built Linux GNU binary; never builds or publishes.
set -eu
if [ "$#" -ne 3 ]; then
    echo 'usage: sh scripts/package-release.sh VERSION BINARY OUTPUT_DIR' >&2
    exit 2
fi
exec python3 - "$@" <<'PY'
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile

try:
    version, binary, output = sys.argv[1:]
    version = version.removeprefix('v')
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version):
        raise ValueError('version must be stable X.Y.Z or vX.Y.Z')
    root = Path.cwd()
    cargo = re.search(r'^version = "([^"]+)"', (root / 'Cargo.toml').read_text(), re.M)[1]
    extension = json.loads((root / 'clients/vscode/package.json').read_text())['version']
    if version != cargo or version != extension:
        raise ValueError('Cargo, extension and requested versions must match')
    binary = Path(binary).resolve()
    if subprocess.check_output([str(binary), '--version'], text=True).strip() != f'magit-lsp {version}':
        raise ValueError('binary version does not match requested version')
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    stem = f'magit-lsp-v{version}-x86_64-unknown-linux-gnu'
    archive = output / (stem + '.tar.gz')
    with tempfile.TemporaryDirectory(prefix='magit-lsp-package-') as temporary:
        notes = Path(temporary) / 'INSTALL.md'
        notes.write_text(
            f'magit-lsp {version}\n\nLinux x86_64 GNU only; glibc >= 2.35 and Git required.\n'
            'Put magit-lsp on PATH (for example ~/.local/bin); no Rust is needed.\n'
            'See docs/RELEASING.md in the source for baseline and verification status.\n'
            'LICENSE is GPL-3.0-only; LICENSE-APACHE covers Apache-2.0-licensed protocol and client components.\n'
        )
        with tarfile.open(archive, 'w:gz') as package:
            for source, name in ((binary, 'magit-lsp'), (root / 'LICENCES/GPL-3.0', 'LICENSE'), (root / 'LICENCES/Apache-2.0', 'LICENSE-APACHE'), (notes, 'INSTALL.md')):
                info = package.gettarinfo(str(source), arcname=f'{stem}/{name}')
                info.mode = 0o755 if name == 'magit-lsp' else 0o644
                info.uid = info.gid = 0
                info.uname = info.gname = ''
                with source.open('rb') as content:
                    package.addfile(info, content)
    checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
    Path(str(archive) + '.sha256').write_text(f'{checksum}  {archive.name}\n')
    print(archive)
except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
    print(f'package-release: {error}', file=sys.stderr)
    sys.exit(1)
PY
