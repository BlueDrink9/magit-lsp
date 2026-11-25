"""Offline release contract tests; optional real-binary package/install/launch smoke."""
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
VERSION = re.search(r'^version = "([^"]+)"', (ROOT / 'Cargo.toml').read_text(), re.M)[1]
TARGET = 'x86_64-unknown-linux-gnu'
DARWIN_TARGET = 'aarch64-apple-darwin'
MEMBER_NAMES = ('magit-lsp', 'LICENSE', 'LICENSE-APACHE', 'INSTALL.md')
STEM = f'magit-lsp-v{VERSION}-{TARGET}'
ASSET = STEM + '.tar.gz'


class ReleaseTests(unittest.TestCase):
    """Exercise scripts through their public CLI with curl/platform mocks on PATH."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='magit-lsp-test-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.assets = self.directory / 'assets'
        self.assets.mkdir()
        self.tools = self.directory / 'tools'
        self.tools.mkdir()
        self.destination = self.directory / 'install space'
        self.destination.mkdir()
        self.target = self.destination / 'magit-lsp'
        self.target.write_text('old binary')
        self.home = self.directory / 'home'
        self.home.mkdir()
        self.env = dict(os.environ, PATH=str(self.tools) + os.pathsep + os.environ['PATH'],
                        HOME=str(self.home), MOCK_ASSETS=str(self.assets),
                        MOCK_LOG=str(self.directory / 'requests'), MOCK_SYSTEM='Linux',
                        MOCK_MACHINE='x86_64', MOCK_LIBC='glibc 2.35')
        for name in ('MOCK_FAIL', 'MOCK_LIBC_FAIL'):
            self.env.pop(name, None)
        self.tool('uname', '''#!/bin/sh
if [ "$1" = -s ]; then printf '%s\\n' "$MOCK_SYSTEM"; else printf '%s\\n' "$MOCK_MACHINE"; fi
''')
        self.tool('getconf', '''#!/bin/sh
printf '%s\\n' "$MOCK_LIBC"
exit "${MOCK_LIBC_FAIL:-0}"
''')
        self.tool('curl', '''#!/usr/bin/env python3
import os, pathlib, shutil, sys
a = sys.argv[1:]
url = a[-1]
assert a[:4] == ['--fail', '--silent', '--show-error', '--location']
assert a[a.index('--proto') + 1] == '=https'
assert a[a.index('--proto-redir') + 1] == '=https'
assert url.startswith('https://')
with open(os.environ['MOCK_LOG'], 'a') as log:
    log.write(url + '\\n')
name = 'latest.json' if url.endswith('/releases/latest') else url.rsplit('/', 1)[1]
if os.environ.get('MOCK_FAIL') == name:
    sys.exit(22)
try:
    shutil.copyfile(pathlib.Path(os.environ['MOCK_ASSETS']) / name, a[a.index('--output') + 1])
except OSError:
    sys.exit(22)
''')
        self.binary = self.directory / 'fake-binary'
        self.binary.write_text(f'#!/bin/sh\nprintf "magit-lsp {VERSION}\\n"\n')
        self.binary.chmod(0o755)
        (self.assets / 'latest.json').write_text(json.dumps({'tag_name': 'v' + VERSION}))
        self.archive()

    def tool(self, name, text):
        path = self.tools / name
        path.write_text(text)
        path.chmod(0o755)

    def archive(self, extra=None, omit=None):
        entries = [(f'{STEM}/magit-lsp', self.binary.read_bytes(), tarfile.REGTYPE),
                   (f'{STEM}/LICENSE', (ROOT / 'LICENCES/GPL-3.0').read_bytes(), tarfile.REGTYPE),
                   (f'{STEM}/LICENSE-APACHE', (ROOT / 'LICENCES/Apache-2.0').read_bytes(), tarfile.REGTYPE),
                   (f'{STEM}/INSTALL.md', b'notes', tarfile.REGTYPE)]
        if omit:
            entries = [entry for entry in entries if not entry[0].endswith('/' + omit)]
        if extra:
            entries.append(extra)
        with tarfile.open(self.assets / ASSET, 'w:gz') as package:
            for name, content, kind in entries:
                info = tarfile.TarInfo(name)
                info.type = kind
                info.size = len(content) if kind == tarfile.REGTYPE else 0
                if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                    info.linkname = '../../outside'
                package.addfile(info, io.BytesIO(content))
        self.checksum()

    def checksum(self):
        digest = hashlib.sha256((self.assets / ASSET).read_bytes()).hexdigest()
        (self.assets / (ASSET + '.sha256')).write_text(f'{digest}  {ASSET}\n')

    def install(self, *arguments, default=False):
        command = ['sh', str(ROOT / 'scripts/install.sh')]
        if not default:
            command += ['--install-dir', str(self.destination)]
        return subprocess.run(command + list(arguments), env=self.env, text=True, capture_output=True)

    def rejected(self, *arguments):
        result = self.install(*arguments)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn('install:', result.stderr)
        self.assertEqual(self.target.read_text(), 'old binary')
        self.assertEqual(list(self.destination.iterdir()), [self.target])
        return result

    def test_latest_and_pinned(self):
        for arguments in ([], ['--version', VERSION], ['--version', 'v' + VERSION]):
            with self.subTest(arguments=arguments):
                result = self.install(*arguments)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.target.read_bytes(), self.binary.read_bytes())
                self.assertTrue(os.access(self.target, os.X_OK))
        urls = (self.directory / 'requests').read_text().splitlines()
        self.assertEqual(urls.count('https://api.github.com/repos/BlueDrink9/magit-lsp/releases/latest'), 1)
        self.assertIn(f'https://github.com/BlueDrink9/magit-lsp/releases/download/v{VERSION}/{ASSET}', urls)

    def test_default_directory(self):
        result = self.install(default=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.home / '.local/bin/magit-lsp').is_file())

    def test_download_failures(self):
        for name in ('latest.json', ASSET, ASSET + '.sha256'):
            with self.subTest(name=name):
                self.env['MOCK_FAIL'] = name
                self.rejected()

    def test_missing_asset(self):
        (self.assets / ASSET).unlink()
        self.rejected('--version', VERSION)

    def test_bad_metadata_and_versions(self):
        for data in ('not json', '{}', '{"tag_name": "../../bad"}', '{"tag_name": null}'):
            with self.subTest(data=data):
                (self.assets / 'latest.json').write_text(data)
                self.rejected()

        for value in ('../bad', '1.2.3;echo bad', '01.2.3', '1.2.3-rc1'):
            self.rejected('--version', value)

    def test_destination_failure_preserves_binary(self):
        self.rejected('--install-dir', str(self.target))

    def test_checksum_failures(self):
        for text in ('bad', '0' * 64 + f'  {ASSET}\n', '0' * 64 + '  ../other\n'):
            with self.subTest(text=text):
                (self.assets / (ASSET + '.sha256')).write_text(text)
                self.rejected()

    def test_platform_failures(self):
        for key, value in (('MOCK_SYSTEM', 'Darwin'), ('MOCK_MACHINE', 'aarch64'),
                           ('MOCK_LIBC', 'musl'), ('MOCK_LIBC', 'glibc 2.34')):
            with self.subTest(key=key, value=value):
                previous = self.env[key]
                self.env[key] = value
                self.rejected()
                self.env[key] = previous
        self.assertFalse((self.directory / 'requests').exists())

    def test_archive_structure(self):
        for extra in (('../outside', b'bad', tarfile.REGTYPE),
                      ('/absolute', b'bad', tarfile.REGTYPE),
                      (f'{STEM}/unexpected', b'bad', tarfile.REGTYPE),
                      (f'{STEM}/magit-lsp', b'duplicate', tarfile.REGTYPE),
                      (f'{STEM}/magit-lsp', b'', tarfile.SYMTYPE),
                      (f'{STEM}/magit-lsp', b'', tarfile.LNKTYPE),
                      (f'{STEM}/directory', b'', tarfile.DIRTYPE)):
            with self.subTest(extra=extra):
                self.archive(extra=extra, omit='magit-lsp' if extra[2] in (tarfile.SYMTYPE, tarfile.LNKTYPE) else None)
                self.rejected()
        self.archive(extra=(f'{STEM}/magit-lsp', b'', tarfile.REGTYPE), omit='magit-lsp')
        self.rejected()
        self.archive(omit='LICENSE')
        self.rejected()
        self.archive(omit='LICENSE-APACHE')
        self.rejected()
        (self.assets / ASSET).write_bytes(b'not a gzip archive')
        self.checksum()
        self.rejected()
        self.archive()
        data = (self.assets / ASSET).read_bytes()
        (self.assets / ASSET).write_bytes(data[:20])
        self.checksum()
        self.rejected()
        self.assertFalse((self.directory / 'outside').exists())

    def write_provenance(self, binary, version=VERSION, target=TARGET, digest=None):
        path = self.directory / 'provenance.json'
        path.write_text(json.dumps({
            'version': version.removeprefix('v'),
            'target': target,
            'executable_sha256': digest or hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
        }))
        return path

    def package(self, binary, version=VERSION, target=TARGET, provenance=None, output=None):
        if provenance is None:
            provenance = self.write_provenance(binary, version=version, target=target)
        return subprocess.run(['sh', str(ROOT / 'scripts/package-release.sh'), version,
                               str(binary), str(output or self.assets), target, str(provenance)],
                              cwd=ROOT, text=True, capture_output=True)

    def test_package_install_contract(self):
        result = self.package(self.binary)
        self.assertEqual(result.returncode, 0, result.stderr)
        archive = (self.assets / ASSET).read_bytes()
        sidecar = (self.assets / (ASSET + '.sha256')).read_text()
        self.assertEqual(sidecar, f'{hashlib.sha256(archive).hexdigest()}  {ASSET}\n')
        with tarfile.open(self.assets / ASSET) as package:
            members = package.getmembers()
            self.assertEqual([member.name for member in members],
                             [f'{STEM}/{name}' for name in MEMBER_NAMES])
            for member in members:
                self.assertTrue(member.isfile(), member.name)
                self.assertEqual((member.uid, member.gid, member.uname, member.gname),
                                 (0, 0, '', ''), member.name)
                self.assertEqual(member.mtime, 0, member.name)
                self.assertEqual(member.mode,
                                 0o755 if member.name.endswith('/magit-lsp') else 0o644, member.name)
            self.assertEqual(package.extractfile(f'{STEM}/LICENSE').read(), (ROOT / 'LICENCES/GPL-3.0').read_bytes())
            self.assertEqual(package.extractfile(f'{STEM}/LICENSE-APACHE').read(), (ROOT / 'LICENCES/Apache-2.0').read_bytes())
        result = self.install('--version', VERSION)
        self.assertEqual(result.returncode, 0, result.stderr)

        # Identical content in a differently named path with a different mtime
        # must produce the same archive and sidecar bytes.
        copy = self.directory / 'another tree' / 'different-name'
        copy.parent.mkdir()
        copy.write_bytes(self.binary.read_bytes())
        copy.chmod(0o644)
        os.utime(self.binary, (1, 1))
        os.utime(copy, (2000000000, 2000000000))
        other = self.directory / 'other output'
        result = self.package(copy, output=other)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((other / ASSET).read_bytes(), archive)
        self.assertEqual((other / (ASSET + '.sha256')).read_text(), sidecar)

        # Changed executable content changes the archive and its sidecar.
        changed = self.directory / 'changed-binary'
        changed.write_bytes(self.binary.read_bytes() + b'\n# changed\n')
        changed.chmod(0o644)
        result = self.package(changed)
        self.assertEqual(result.returncode, 0, result.stderr)
        changed_archive = (self.assets / ASSET).read_bytes()
        self.assertNotEqual(changed_archive, archive)
        self.assertEqual((self.assets / (ASSET + '.sha256')).read_text(),
                         f'{hashlib.sha256(changed_archive).hexdigest()}  {ASSET}\n')

        # Packaging never executes the input binary, even a non-executable one.
        marker = self.directory / 'binary-was-executed'
        sentinel = self.directory / 'sentinel'
        sentinel.write_text(f'#!/bin/sh\ntouch "{marker}"\nexit 1\n')
        sentinel.chmod(0o644)
        result = self.package(sentinel)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(marker.exists())

        # Another supported target names its members from that target, with no
        # implicit Windows executable suffix.
        darwin = self.directory / 'darwin output'
        result = self.package(self.binary, target=DARWIN_TARGET, output=darwin)
        self.assertEqual(result.returncode, 0, result.stderr)
        darwin_stem = f'magit-lsp-v{VERSION}-{DARWIN_TARGET}'
        with tarfile.open(darwin / (darwin_stem + '.tar.gz')) as package:
            self.assertEqual(package.getnames(),
                             [f'{darwin_stem}/{name}' for name in MEMBER_NAMES])

    def test_package_rejects_version_mismatch(self):
        # The requested version still has to match Cargo and the extension.
        self.assertNotEqual(self.package(self.binary, '99.0.0').returncode, 0)

        # Provenance is mandatory and must agree with the requested identity.
        absent = self.directory / 'missing-provenance.json'
        self.assertNotEqual(self.package(self.binary, provenance=absent).returncode, 0)
        for changes in ({'version': '0.0.1'}, {'target': DARWIN_TARGET},
                        {'digest': '0' * 64}):
            provenance = self.write_provenance(self.binary, **changes)
            with self.subTest(changes=changes):
                self.assertNotEqual(self.package(self.binary, provenance=provenance).returncode, 0)
        for content in ('not json', '[]', '{}',
                        json.dumps({'version': VERSION, 'target': TARGET}),
                        json.dumps({'version': VERSION, 'target': TARGET,
                                    'executable_sha256': 'A' * 64})):
            provenance = self.directory / 'malformed-provenance.json'
            provenance.write_text(content)
            with self.subTest(content=content):
                self.assertNotEqual(self.package(self.binary, provenance=provenance).returncode, 0)

        # Unsupported or malicious target names are refused before any write.
        # x86_64-apple-darwin is retired; it must keep being refused.
        for target in ('x86_64-apple-darwin', 'x86_64-pc-windows-msvc',
                       'x86_64-unknown-linux-gnu/..',
                       '../x86_64-unknown-linux-gnu', '', 'AMD64'):
            with self.subTest(target=target):
                self.assertNotEqual(self.package(self.binary, target=target).returncode, 0)

        # Stale provenance cannot cover a replaced executable.
        provenance = self.write_provenance(self.binary)
        self.binary.write_bytes(b'#!/bin/sh\necho replacement\n')
        self.assertNotEqual(self.package(self.binary, provenance=provenance).returncode, 0)

    @unittest.skipUnless(os.environ.get('MAGIT_LSP_BIN'), 'set MAGIT_LSP_BIN for actual-artifact smoke')
    def test_real_binary_package_install_launch(self):
        binary = Path(os.environ['MAGIT_LSP_BIN']).resolve()
        result = self.package(binary)
        self.assertEqual(result.returncode, 0, result.stderr)
        result = self.install('--version', VERSION)
        self.assertEqual(result.returncode, 0, result.stderr)
        launched = subprocess.run([str(self.target), '--version'], capture_output=True, text=True, timeout=15)
        self.assertEqual(launched.returncode, 0, launched.stderr)
        self.assertEqual(launched.stdout, f'magit-lsp {VERSION}\n')
