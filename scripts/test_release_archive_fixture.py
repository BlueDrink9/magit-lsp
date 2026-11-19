#!/usr/bin/env python3
"""Write a release archive exactly as scripts/package-release.sh does.

Used by clients/vscode/test/server-download.test.js. Python's default tarfile
format emits a `././@PaxHeader` extended header for the fractional mtime a
release build has, so this fixture must be produced here instead of hand-writing
ustar bytes. Usage: test_release_archive_fixture.py ARCHIVE STEM BINARY
"""
from __future__ import annotations

import sys
import tarfile
import tempfile
from pathlib import Path

MTIME = 1700000000.125


def main() -> None:
    """Write ARCHIVE with the published member set and a fractional mtime."""
    if len(sys.argv) != 4:
        raise SystemExit('usage: test_release_archive_fixture.py ARCHIVE STEM BINARY')
    archive, stem, binary = Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3])
    with tempfile.TemporaryDirectory(prefix='magit-lsp-fixture-') as temporary:
        notes = Path(temporary) / 'INSTALL.md'
        notes.write_text('magit-lsp fixture\n')
        licences = []
        for name, text in (('LICENSE', 'GPL-3.0-only\n'), ('LICENSE-APACHE', 'Apache-2.0\n')):
            path = Path(temporary) / name
            path.write_text(text)
            licences.append((path, name))
        with tarfile.open(archive, 'w:gz') as package:
            for source, name in ((binary, 'magit-lsp'), *licences, (notes, 'INSTALL.md')):
                info = package.gettarinfo(str(source), arcname=f'{stem}/{name}')
                info.mode = 0o755 if name == 'magit-lsp' else 0o644
                info.uid = info.gid = 0
                info.uname = info.gname = ''
                # Never rely on filesystem mtime precision: the packaged binary
                # has a fractional mtime, which is what emits the pax header.
                info.mtime = MTIME
                with source.open('rb') as content:
                    package.addfile(info, content)


if __name__ == '__main__':
    main()
