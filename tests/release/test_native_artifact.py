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
                info.mode = 0o755 if name == 'magit-lsp' else 0o644
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
                'arm64', 'cmd LC_BUILD_VERSION\n platform 1\n minos 11.0\n path @rpath/lib.dylib',
                'binary:\n\t/usr/lib/libSystem.B.dylib (compatibility version 1)']):
            evidence = {}
            inspector.inspect_macho(Path('binary'), target, evidence)
            self.assertEqual(evidence['deployment'], '11.0')
            self.assertEqual(evidence['dylibs'], ['/usr/lib/libSystem.B.dylib'])
            self.assertEqual(evidence['rpaths'], ['@rpath/lib.dylib'])
        with mock.patch.object(inspector, 'run_tool', side_effect=['x86_64']):
            with self.assertRaisesRegex(inspector.InspectionError, 'architecture'):
                inspector.inspect_macho(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=['arm64 x86_64']):
            with self.assertRaisesRegex(inspector.InspectionError, 'architecture'):
                inspector.inspect_macho(Path('binary'), target, {})
        for selected_target, arch, accepted in [
                (target, 'arm64', ('10.12', '11.0'))]:
            for deployment in accepted:
                with mock.patch.object(inspector, 'run_tool', side_effect=[
                        arch, f'cmd LC_BUILD_VERSION\n platform 1\n minos {deployment}',
                        'binary:\n /usr/lib/libSystem.B.dylib']):
                    inspector.inspect_macho(Path('binary'), selected_target, {})
            later = '11.1'
            with mock.patch.object(inspector, 'run_tool', side_effect=[
                    arch, f'cmd LC_BUILD_VERSION\n platform 1\n minos {later}',
                    'binary:\n /usr/lib/libSystem.B.dylib']):
                with self.assertRaisesRegex(inspector.InspectionError, 'exceeds target baseline'):
                    inspector.inspect_macho(Path('binary'), selected_target, {})
        for load_commands in [
                'cmd LC_BUILD_VERSION\n minos 11.0',
                'cmd LC_BUILD_VERSION\n platform 2\n minos 11.0',
                'cmd LC_BUILD_VERSION\n platform 1\n minos 11.0\nLoad command 2\n cmd LC_VERSION_MIN_MACOSX\n version 10.12']:
            with mock.patch.object(inspector, 'run_tool', side_effect=[
                    'arm64', load_commands, 'binary:\n /usr/lib/libSystem.B.dylib']):
                with self.assertRaises(inspector.InspectionError):
                    inspector.inspect_macho(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'arm64', 'cmd LC_BUILD_VERSION\n platform 1\n minos nonsense', 'binary:\n /nix/store/lib']):
            with self.assertRaises(inspector.InspectionError):
                inspector.inspect_macho(Path('binary'), target, {})
        with mock.patch.object(inspector, 'run_tool', side_effect=[
                'arm64', 'cmd LC_BUILD_VERSION\n platform 1\n minos 11.0\n path /nix/store/lib',
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
                'Shared library: [libc.so.6]', 'GLIBC_2.35']), \
                mock.patch.object(inspector.subprocess, 'Popen', side_effect=AssertionError('static inspector executed')):
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
                                         ('release/magit-lsp', tarfile.SYMTYPE),
                                         ('release/magit-lsp', tarfile.LNKTYPE)]:
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

        archive = self.root / 'unexpected.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            for name in ('magit-lsp', 'LICENSE', 'LICENSE-APACHE', 'INSTALL.md', 'unexpected'):
                info = tarfile.TarInfo('release/' + name)
                info.size = 1
                tar.addfile(info, io.BytesIO(b'x'))
        with self.assertRaisesRegex(inspector.InspectionError,
                                    'archive members do not match'):
            inspector.safe_extract(archive, self.root / 'extract-unexpected',
                                   archive.name.removesuffix('.tar.gz'))

    def test_safe_extract_works_without_tarfile_filter_api_and_preserves_executable(self):
        archive = self.candidate()
        destination = self.root / 'extract'
        with mock.patch.object(tarfile.TarFile, 'extractall',
                               side_effect=TypeError('filter argument unavailable')):
            executable = inspector.safe_extract(archive, destination,
                archive.name.removesuffix('.tar.gz'))
        self.assertEqual(executable.read_bytes(), b'ELF')
        self.assertEqual(executable.stat().st_mode & 0o777, 0o755)

    def test_missing_tool(self):
        with mock.patch.dict(os.environ, {'PATH': str(self.tools)}):
            with self.assertRaisesRegex(inspector.InspectionError, 'inspection tool failed'):
                inspector.run_tool(['native-inspector-tool-that-does-not-exist'])

    def test_relative_candidate_is_refused_before_path_resolution_or_execution(self):
        with mock.patch.object(Path, 'resolve', side_effect=AssertionError('resolved relative input')), \
                mock.patch.object(inspector.subprocess, 'Popen', side_effect=AssertionError('executed')):
            result = inspector.inspect('relative-candidate.tar.gz', functional=True)
        self.assertFalse(result['ok'])
        self.assertIn('not absolute', result['error'])

    def test_functional_host_mismatch_precedes_binary_launch(self):
        with mock.patch.object(inspector.platform, 'system', return_value='Darwin'), \
                mock.patch.object(inspector.platform, 'machine', return_value='arm64'), \
                mock.patch.object(inspector.subprocess, 'run', side_effect=AssertionError('launched')):
            with self.assertRaisesRegex(inspector.InspectionError, 'does not match target'):
                inspector._functional(Path('/exact/extracted/binary'),
                                      'x86_64-unknown-linux-gnu', '1.2.3', {})

    def test_functional_rejects_manifest_version_mismatch_before_server_launch(self):
        result = subprocess.CompletedProcess([], 0, stdout='magit-lsp 9.9.9\n', stderr='')
        with mock.patch.object(inspector.platform, 'system', return_value='Linux'), \
                mock.patch.object(inspector.platform, 'machine', return_value='x86_64'), \
                mock.patch.object(inspector.subprocess, 'run', return_value=result), \
                mock.patch.object(inspector.subprocess, 'Popen', side_effect=AssertionError('launched')):
            with self.assertRaisesRegex(inspector.InspectionError, 'does not match manifest version'):
                inspector._functional(Path('/exact/extracted/binary'),
                                      'x86_64-unknown-linux-gnu', '1.2.3', {})

    def test_rpc_acknowledges_structured_render_before_matching_response(self):
        render = {'jsonrpc': '2.0', 'method': 'view/render',
                  'params': {'viewId': 'status-1', 'version': 7,
                             'text': ['Unstaged changes'], 'mode': 'interactive'}}
        response = {'jsonrpc': '2.0', 'id': 8, 'result': {'effects': []}}
        stream = b''.join(b'Content-Length: %d\r\n\r\n' % len(payload) + payload
                          for payload in (json.dumps(render).encode(), json.dumps(response).encode()))
        read_fd, write_fd = os.pipe()
        try:
            os.write(write_fd, stream)
            os.close(write_fd)
            process = type('FakeProcess', (), {})()
            process.stdin = io.BytesIO()
            process.stdout = os.fdopen(read_fd, 'rb', buffering=0)
            result = inspector._rpc(process, 'view/invoke', {'command': 'status.stage-all'}, 8)
            self.assertEqual(result, {'effects': []})
            process._native_status_view = {'id': 'status-1', 'mode': 'interactive'}
            self.assertTrue(inspector._ack_status_render(process, render['params']))
            self.assertEqual(process._native_render_count, 1)
            self.assertEqual(process._native_render_version, 7)
            ack_frame = process.stdin.getvalue()
            ack = ack_frame.split(b'\r\n\r\n', 1)[1]
            ack = ack[:int(ack_frame.split(b'\r\n', 1)[0].split(b':', 1)[1])]
            outgoing = []
            while ack_frame:
                head, rest = ack_frame.split(b'\r\n\r\n', 1)
                size = int(head.split(b':', 1)[1])
                outgoing.append(json.loads(rest[:size]))
                ack_frame = rest[size:]
            ack = next(message for message in outgoing if message.get('method') == 'view/rendered')
            self.assertEqual(ack['params'], {'viewId': 'status-1', 'version': 7})
        finally:
            try:
                os.close(write_fd)
            except OSError:
                pass
            process.stdout.close()

    def test_functional_probe_launches_exact_binary_and_asserts_git_index(self):
        def frame(message):
            payload = json.dumps(message).encode()
            return b'Content-Length: %d\r\n\r\n' % len(payload) + payload

        messages = [
            {'jsonrpc': '2.0', 'id': 1, 'result': {'capabilities': {'experimental': {
                'magit': {'protocolVersion': 1}}}}},
            {'jsonrpc': '2.0', 'method': 'view/keymap', 'params': {'viewId': 's'}},
            {'jsonrpc': '2.0', 'method': 'view/render', 'params': {
                'viewId': 's', 'version': 1, 'text': ['Unstaged changes'],
                'mode': 'interactive'}},
            {'jsonrpc': '2.0', 'id': 2, 'result': {'id': 's', 'kind': 'status',
                                                    'mode': 'interactive'}},
            {'jsonrpc': '2.0', 'id': 3, 'result': {'effects': []}},
            {'jsonrpc': '2.0', 'id': 4, 'result': None},
        ]

        class Capture(io.BytesIO):
            def close(self):
                pass

        class Child:
            def __init__(self):
                self.stdin = Capture()
                self.read_fd, self.write_fd = os.pipe()
                os.write(self.write_fd, b''.join(map(frame, messages)))
                os.close(self.write_fd)
                self.stdout = os.fdopen(self.read_fd, 'rb', buffering=0)
                self.alive = True

            def poll(self):
                return None if self.alive else 0

            def terminate(self):
                self.alive = False

            def wait(self, timeout=None):
                self.alive = False

            def kill(self):
                self.alive = False

        child = Child()
        calls = []
        def fake_run(args, **kwargs):
            calls.append(args)
            output = 'magit-lsp 1.2.3\n' if args == ['/tmp/extracted exact/magit-lsp', '--version'] else (
                'tracked.txt\n' if args[-3:] == ['diff', '--cached', '--name-only'] else '')
            return subprocess.CompletedProcess(args, 0, stdout=output, stderr='')

        evidence = {}
        try:
            with mock.patch.object(inspector.platform, 'system', return_value='Linux'), \
                    mock.patch.object(inspector.platform, 'machine', return_value='x86_64'), \
                    mock.patch.object(inspector.subprocess, 'run', side_effect=fake_run), \
                    mock.patch.object(inspector.subprocess, 'Popen', return_value=child) as popen:
                inspector._functional(Path('/tmp/extracted exact/magit-lsp'),
                                      'x86_64-unknown-linux-gnu', '1.2.3', evidence)
            self.assertEqual(popen.call_args.args[0], ['/tmp/extracted exact/magit-lsp'])
            sent = child.stdin.getvalue()
            self.assertIn(b'"v": 1', sent)
            self.assertIn(b'"method": "view/rendered"', sent)
            self.assertIn(b'"method": "view/invoke"', sent)
            self.assertIn(b'"command": "status.stage-all"', sent)
            self.assertIn(['/tmp/extracted exact/magit-lsp', '--version'], calls)
            self.assertEqual(evidence['git']['staged'], ['tracked.txt'])
            self.assertEqual(evidence['lsp']['status_render_acks'], 1)
            self.assertEqual(evidence['teardown'], 'complete')
        finally:
            child.stdout.close()

    def test_functional_rejects_invalid_negotiation_descriptor_and_render_before_mutation(self):
        def frame(message):
            payload = json.dumps(message).encode()
            return b'Content-Length: %d\r\n\r\n' % len(payload) + payload

        class Child:
            def __init__(self, messages):
                self.stdin = io.BytesIO()
                read_fd, write_fd = os.pipe()
                os.write(write_fd, b''.join(map(frame, messages)))
                os.close(write_fd)
                self.stdout = os.fdopen(read_fd, 'rb', buffering=0)
            def poll(self): return 0
            def terminate(self): pass
            def wait(self, timeout=None): pass
            def kill(self): pass

        base = [
            {'jsonrpc': '2.0', 'id': 1, 'result': {'capabilities': {'experimental': {
                'magit': {'protocolVersion': 1}}}}},
            {'jsonrpc': '2.0', 'method': 'view/render', 'params': {
                'viewId': 's', 'version': 1, 'text': ['status'], 'mode': 'interactive'}},
            {'jsonrpc': '2.0', 'id': 2, 'result': {'id': 's', 'kind': 'status',
                                                    'mode': 'interactive'}},
            {'jsonrpc': '2.0', 'id': 3, 'result': {'effects': []}},
            {'jsonrpc': '2.0', 'id': 4, 'result': None},
        ]
        cases = [
            ('version', lambda msgs: msgs[0]['result']['capabilities']['experimental']['magit']
             .update(protocolVersion=2), 'protocol version 1'),
            ('kind', lambda msgs: msgs[2]['result'].update(kind='log'), 'invalid status descriptor'),
            ('view', lambda msgs: msgs[1]['params'].update(viewId='other'),
             'no valid matching full render'),
            ('payload', lambda msgs: msgs[1]['params'].update(text=None),
             'no valid matching full render'),
        ]
        for _, alter, error in cases:
            messages = json.loads(json.dumps(base))
            alter(messages)
            child = Child(messages)
            calls = []
            def fake_run(args, **kwargs):
                calls.append(args)
                output = 'magit-lsp 1.2.3\n' if args[-1:] == ['--version'] else ''
                return subprocess.CompletedProcess(args, 0, stdout=output, stderr='')
            try:
                with mock.patch.object(inspector.platform, 'system', return_value='Linux'), \
                        mock.patch.object(inspector.platform, 'machine', return_value='x86_64'), \
                        mock.patch.object(inspector.subprocess, 'run', side_effect=fake_run), \
                        mock.patch.object(inspector.subprocess, 'Popen', return_value=child):
                    with self.assertRaisesRegex(inspector.InspectionError, error):
                        inspector._functional(Path('/tmp/extracted'),
                                              'x86_64-unknown-linux-gnu', '1.2.3', {})
                self.assertFalse(any(args[-3:] == ['diff', '--cached', '--name-only']
                                     for args in calls))
            finally:
                child.stdout.close()

    def test_shutdown_failure_still_reaps_child_and_records_evidence(self):
        class Child:
            def __init__(self):
                self.alive = True
                self.killed = False

            def poll(self):
                return None if self.alive else 0

            def terminate(self):
                pass

            def wait(self, timeout=None):
                if not self.killed:
                    raise subprocess.TimeoutExpired('server', timeout)
                self.alive = False

            def kill(self):
                self.killed = True

        child = Child()
        evidence = {}
        with mock.patch.object(inspector, '_rpc', side_effect=inspector.InspectionError('timeout')):
            inspector._stop_process(child, evidence)
        self.assertEqual(evidence['shutdown_error'], 'timeout')
        self.assertFalse(evidence['children_remaining'])
        self.assertTrue(child.killed)

    def test_functional_rpc_timeout_still_tears_down_owned_child(self):
        class Child:
            def __init__(self):
                self.stdin = io.BytesIO()
                self.stdout = io.BytesIO()
                self.alive = True

            def poll(self):
                return None if self.alive else 0

            def terminate(self):
                self.alive = False

            def wait(self, timeout):
                self.alive = False

            def kill(self):
                self.alive = False

        child = Child()
        run_calls = []
        def fake_run(args, **kwargs):
            run_calls.append(args)
            return subprocess.CompletedProcess(args, 0, stdout='magit-lsp 1.2.3\n', stderr='')

        evidence = {}
        with mock.patch.object(inspector.platform, 'system', return_value='Linux'), \
                mock.patch.object(inspector.platform, 'machine', return_value='x86_64'), \
                mock.patch.object(inspector.subprocess, 'run', side_effect=fake_run), \
                mock.patch.object(inspector.subprocess, 'Popen', return_value=child), \
                mock.patch.object(inspector, '_rpc', side_effect=inspector.InspectionError('timed out')):
            with self.assertRaisesRegex(inspector.InspectionError, 'timed out'):
                inspector._functional(Path('/tmp/extracted/magit-lsp'),
                                      'x86_64-unknown-linux-gnu', '1.2.3', evidence)
        self.assertTrue(any(args[-1:] == ['--version'] for args in run_calls))
        self.assertFalse(child.alive)
        self.assertFalse(evidence['children_remaining'])
        self.assertEqual(evidence['teardown'], 'complete')


if __name__ == '__main__':
    unittest.main()
