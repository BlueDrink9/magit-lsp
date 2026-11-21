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

    def test_elf_rejects_every_high_symbol_even_after_benign_symbols(self):
        self.fake('readelf', 'Advanced Micro Devices X86-64\nRequesting program interpreter: /lib64/ld-linux-x86-64.so.2\nShared library: [libc.so.6]\nGLIBC_2.2 GLIBC_2.35 GLIBC_2.36')
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'Advanced Micro Devices X86-64', 'Requesting program interpreter: /lib64/ld.so',
                'Shared library: [libc.so.6]', 'GLIBC_2.2 GLIBC_2.36']):
            with self.assertRaisesRegex(inspector.InspectionError, 'exceeds 2.35'):
                inspector.inspect_elf(Path('binary'), inspector.TARGETS['x86_64-unknown-linux-gnu'], {})

    def test_elf_rejects_wrong_architecture_missing_dependency_and_cxx_abi(self):
        target = inspector.TARGETS['x86_64-unknown-linux-gnu']
        with mock.patch.object(inspector, 'run_tool', side_effect=['AArch64']):
            with self.assertRaisesRegex(inspector.InspectionError, 'architecture'):
                inspector.inspect_elf(Path('binary'), target, {})
        prefix = ['Advanced Micro Devices X86-64',
                  'Requesting program interpreter: /lib64/ld.so']
        with mock.patch.object(inspector, 'run_tool', side_effect=prefix +
                               ['no needed libraries', 'GLIBC_2.2']):
            with self.assertRaisesRegex(inspector.InspectionError, 'libraries'):
                inspector.inspect_elf(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=prefix +
                               ['Shared library: [libc.so.6]', 'GLIBC_2.2 GLIBCXX_3.4']):
            with self.assertRaisesRegex(inspector.InspectionError, 'GLIBCXX/CXXABI'):
                inspector.inspect_elf(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'Advanced Micro Devices X86-64', 'Requesting program interpreter: /lib64/ld.so',
                'RUNPATH [/nix/store/x]', 'GLIBC_2.2']):
            with self.assertRaisesRegex(inspector.InspectionError, 'Nix'):
                inspector.inspect_elf(Path('binary'), inspector.TARGETS['x86_64-unknown-linux-gnu'], {})

    def test_macho_deployment_architecture_malformed_value_and_nix_paths(self):
        target = inspector.TARGETS['aarch64-apple-darwin']
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'arm64', 'cmd LC_BUILD_VERSION\n minos 11.0\n path @rpath/lib.dylib',
                'binary:\n\t/usr/lib/libSystem.B.dylib (compatibility version 1)']):
            evidence = {}
            inspector.inspect_macho(Path('binary'), target, evidence)
            self.assertEqual(evidence['deployment'], '11.0')
            self.assertEqual(evidence['dylibs'], ['/usr/lib/libSystem.B.dylib'])
            self.assertEqual(evidence['rpaths'], ['@rpath/lib.dylib'])
        with mock.patch.object(inspector, 'run_tool', side_effect=['x86_64']):
            with self.assertRaisesRegex(inspector.InspectionError, 'architecture'):
                inspector.inspect_macho(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'arm64', 'cmd LC_BUILD_VERSION\n minos nonsense', 'binary:\n /nix/store/lib']):
            with self.assertRaises(inspector.InspectionError):
                inspector.inspect_macho(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'arm64', 'cmd LC_BUILD_VERSION\n minos 11.0\n path /nix/store/lib',
                'binary:\n\t/usr/lib/libSystem.B.dylib (compatibility version 1)']):
            with self.assertRaisesRegex(inspector.InspectionError, 'Nix'):
                inspector.inspect_macho(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'arm64', 'cmd LC_VERSION_MIN_MACOSX\n version 10',
                'binary:\n\t/usr/lib/libSystem.B.dylib (compatibility version 1)']):
            with self.assertRaisesRegex(inspector.InspectionError, 'deployment version'):
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
        (self.root / (archive.name + '.sha256')).write_text('0' * 64 + '  ' + archive.name + '\n')
        self.assertIn('archive checksum sidecar mismatch', inspector.inspect(archive)['error'])

    def test_candidate_rejects_manifest_and_executable_hash_tampering(self):
        archive = self.candidate()
        manifest_path = self.root / 'release-manifest.json'
        manifest_path.write_text(manifest_path.read_text().replace('1.2.3', '1.2.4'))
        self.assertIn('manifest checksum mismatch', inspector.inspect(archive)['error'])
        archive = self.candidate()
        manifest = json.loads(manifest_path.read_text())
        manifest['executable_sha256'] = '0' * 64
        manifest_path.write_text(json.dumps(manifest))
        (self.root / 'release-manifest.json.sha256').write_text(
            f'{hashlib.sha256(manifest_path.read_bytes()).hexdigest()}  release-manifest.json\n')
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'Advanced Micro Devices X86-64', 'Requesting program interpreter: /lib64/ld.so',
                'Shared library: [libc.so.6]', 'GLIBC_2.35']):
            self.assertIn('executable hash does not match', inspector.inspect(archive)['error'])

    def test_safe_extract_rejects_traversal_and_links(self):
        for member_name, member_type in [('release/../outside', tarfile.REGTYPE),
                                         ('release/magit-lsp', tarfile.SYMTYPE)]:
            archive = self.root / 'unsafe.tar.gz'
            with tarfile.open(archive, 'w:gz') as tar:
                info = tarfile.TarInfo(member_name)
                info.type = member_type
                info.linkname = '../../outside'
                info.size = 1 if member_type == tarfile.REGTYPE else 0
                tar.addfile(info, io.BytesIO(b'x'))
            with self.subTest(member=member_name), self.assertRaisesRegex(
                    inspector.InspectionError, 'unsafe archive member'):
                inspector.safe_extract(archive, self.root / 'extract', 'release')

    def test_missing_tool(self):
        with mock.patch.dict(os.environ, {'PATH': str(self.tools)}):
            with self.assertRaisesRegex(inspector.InspectionError, 'inspection tool failed'):
                inspector.run_tool(['native-inspector-tool-that-does-not-exist'])


if __name__ == '__main__':
    unittest.main()
