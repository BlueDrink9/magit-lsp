"""Static native artifact inspection tests with synthetic binaries and tools."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock

from scripts import check_native_artifact as inspector


class NativeArtifactTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.tools = self.root / 'tools'
        self.tools.mkdir()

    def candidate(self, target='x86_64-unknown-linux-gnu', binary=b'ELF'):
        version = '1.2.3'
        stem = f'magit-lsp-v{version}-{target}'
        archive = self.root / (stem + '.tar.gz')
        with tarfile.open(archive, 'w:gz') as tar:
            for name, content in [('magit-lsp', binary), ('LICENSE', b'a'),
                                  ('LICENSE-APACHE', b'b'), ('INSTALL.md', b'n')]:
                info = tarfile.TarInfo(stem + '/' + name)
                info.size = len(content)
                tar.addfile(info, io.BytesIO(content))
        ah = hashlib.sha256(archive.read_bytes()).hexdigest()
        (self.root / (archive.name + '.sha256')).write_text(f'{ah}  {archive.name}\n')
        manifest = {'version': version, 'target': target, 'archive_sha256': ah,
                    'executable_sha256': hashlib.sha256(binary).hexdigest()}
        mp = self.root / 'release-manifest.json'
        mp.write_text(json.dumps(manifest))
        (self.root / 'release-manifest.json.sha256').write_text(
            f'{hashlib.sha256(mp.read_bytes()).hexdigest()}  release-manifest.json\n')
        return archive

    def fake(self, name, output):
        path = self.tools / name
        path.write_text('#!/bin/sh\nprintf %s ' + repr(output) + '\n')
        path.chmod(0o755)

    def test_elf_symbol_scan_and_path(self):
        self.fake('readelf', 'Advanced Micro Devices X86-64\nRequesting program interpreter: /lib64/ld-linux-x86-64.so.2\nShared library: [libc.so.6]\nGLIBC_2.2 GLIBC_2.35 GLIBC_2.36')
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'Advanced Micro Devices X86-64', 'Requesting program interpreter: /lib64/ld.so',
                'Shared library: [libc.so.6]', 'GLIBC_2.2 GLIBC_2.36']):
            with self.assertRaisesRegex(inspector.InspectionError, 'exceeds 2.35'):
                inspector.inspect_elf(Path('binary'), inspector.TARGETS['x86_64-unknown-linux-gnu'], {})
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'Advanced Micro Devices X86-64', 'Requesting program interpreter: /lib64/ld.so',
                'RUNPATH [/nix/store/x]', 'GLIBC_2.2']):
            with self.assertRaisesRegex(inspector.InspectionError, 'Nix'):
                inspector.inspect_elf(Path('binary'), inspector.TARGETS['x86_64-unknown-linux-gnu'], {})

    def test_macho_deployment_and_rejections(self):
        target = inspector.TARGETS['aarch64-apple-darwin']
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'arm64', 'cmd LC_BUILD_VERSION\n minos 11.0\n path @rpath/lib.dylib',
                'binary:\n\t/usr/lib/libSystem.B.dylib (compatibility version 1)']):
            evidence = {}
            inspector.inspect_macho(Path('binary'), target, evidence)
            self.assertEqual(evidence['deployment'], '11.0')
        with mock.patch.object(inspector, 'run_tool', side_effect=['x86_64']):
            with self.assertRaisesRegex(inspector.InspectionError, 'architecture'):
                inspector.inspect_macho(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'arm64', 'cmd LC_BUILD_VERSION\n minos nonsense', 'binary:\n /nix/store/lib']):
            with self.assertRaises(inspector.InspectionError):
                inspector.inspect_macho(Path('binary'), target, {})

    def test_candidate_identity_archive_safety_and_missing_input(self):
        self.fake('readelf', 'unused')
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'Advanced Micro Devices X86-64', 'Requesting program interpreter: /lib64/ld.so',
                'Shared library: [libc.so.6]', 'GLIBC_2.35']):
            result = inspector.inspect(self.candidate())
        self.assertTrue(result['ok'], result)
        self.assertFalse(inspector.inspect(str(self.root / 'absent'))['ok'])
        archive = self.candidate(binary=b'tampered')
        data = archive.read_bytes().replace(b'magit-lsp', b'../bad--lsp', 1)
        archive.write_bytes(data)
        self.assertFalse(inspector.inspect(archive)['ok'])

    def test_missing_tool(self):
        with mock.patch.object(inspector.shutil, 'which', return_value=None):
            with mock.patch.object(inspector, 'run_tool', side_effect=inspector.InspectionError('missing tool')):
                with self.assertRaisesRegex(inspector.InspectionError, 'missing tool'):
                    inspector.run_tool(['not-installed'])


if __name__ == '__main__':
    unittest.main()
