"""Offline contract tests for immutable native candidate handoff."""
import hashlib
import email.message
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import urllib.error
import zipfile
from unittest.mock import patch

from scripts import download_native_candidate as downloader


class Response:
    def __init__(self, data):
        self.data = data

    def read(self):
        return self.data

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False


def zipped(files):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, 'w') as archive:
        for name, data in files.items():
            archive.writestr(name, data)
    return stream.getvalue()


class NativeCandidateDownloadTests(unittest.TestCase):
    def setUp(self):
        self.run_id, self.attempt = '101', '2'
        self.source = 'a' * 40
        self.target = 'x86_64-unknown-linux-gnu'
        self.name = 'native-linux-x86_64-gnu-experimental'
        binary = b'not executed'
        self.version = '1.2.3'
        self.archive_name = f'magit-lsp-v{self.version}-{self.target}.tar.gz'
        tar = io.BytesIO()
        with tarfile.open(fileobj=tar, mode='w:gz') as package:
            info = tarfile.TarInfo(f'{self.archive_name[:-7]}/magit-lsp')
            info.mode, info.size = 0o755, len(binary)
            package.addfile(info, io.BytesIO(binary))
            for name in ('LICENSE', 'LICENSE-APACHE', 'INSTALL.md'):
                data = b'license'
                info = tarfile.TarInfo(f'{self.archive_name[:-7]}/{name}')
                info.size = len(data)
                package.addfile(info, io.BytesIO(data))
        archive_bytes = tar.getvalue()
        manifest = {'source_sha': self.source, 'run_id': self.run_id,
                    'run_attempt': self.attempt, 'target': self.target,
                    'version': self.version, 'archive_sha256': hashlib.sha256(archive_bytes).hexdigest(),
                    'executable_sha256': hashlib.sha256(binary).hexdigest()}
        manifest_bytes = (json.dumps(manifest) + '\n').encode()
        outer = zipped({self.archive_name: archive_bytes,
                        self.archive_name + '.sha256': (hashlib.sha256(archive_bytes).hexdigest() + '  ' + self.archive_name + '\n').encode(),
                        'release-manifest.json': manifest_bytes,
                        'release-manifest.json.sha256': (hashlib.sha256(manifest_bytes).hexdigest() + '\n').encode()})
        self.candidate = {'id': 22, 'name': self.name, 'digest': hashlib.sha256(outer).hexdigest(),
                          'expired': False, 'workflow_run': {'id': self.run_id}}
        self.identity = {'id': 23, 'name': self.name + '-identity', 'expired': False}
        self.identity_zip = zipped({'upload-identity.json': json.dumps({
            'artifact_id': '22', 'artifact_digest': self.candidate['digest'],
            'run_id': self.run_id, 'run_attempt': self.attempt, 'target': self.target}).encode()})
        self.outer = outer

    def execute(self, *, identity=None, candidate=None, payload=None, output=None):
        candidate = candidate or self.candidate
        identity = identity or self.identity
        payload = self.outer if payload is None else payload
        responses = {
            f'{downloader.API}/repos/o/r/actions/runs/{self.run_id}': json.dumps({
                'id': self.run_id, 'run_attempt': self.attempt, 'head_sha': self.source,
                'repository': {'full_name': 'o/r'}}).encode(),
            f'{downloader.API}/repos/o/r/actions/runs/{self.run_id}/artifacts?per_page=100':
                json.dumps({'artifacts': [identity, candidate]}).encode(),
            f'{downloader.API}/repos/o/r/actions/artifacts/23/zip': self.identity_zip,
            f'{downloader.API}/repos/o/r/actions/artifacts/22': json.dumps(candidate).encode(),
            f'{downloader.API}/repos/o/r/actions/artifacts/22/zip': payload,
        }
        def fake_request(url, token, **_):
            if url not in responses:
                raise AssertionError(url)
            return Response(responses[url])
        with tempfile.TemporaryDirectory() as temp, patch.object(downloader, 'request', fake_request):
            return downloader.fetch_candidate('o/r', self.run_id, self.attempt, self.source,
                self.target, self.name, output or Path(temp) / 'out', 'secret', opener=downloader.request)

    def test_validates_immutable_artifact_and_emits_handoff(self):
        with tempfile.TemporaryDirectory() as temp:
            result = self.execute(output=Path(temp) / 'out')
            self.assertEqual(result['artifact_id'], '22')
            self.assertTrue(Path(result['archive']).is_file())
            self.assertTrue(Path(result['manifest']).is_file())
            self.assertTrue((Path(temp) / 'out' / 'native-candidate.json').is_file())
            self.assertEqual(result['archive_sha256'], hashlib.sha256(Path(result['archive']).read_bytes()).hexdigest())
            manifest = json.loads(Path(result['manifest']).read_text())
            self.assertEqual((manifest['source_sha'], manifest['run_id'], manifest['run_attempt'], manifest['target']),
                             (self.source, self.run_id, self.attempt, self.target))
            self.assertEqual(result['executable_sha256'], hashlib.sha256(b'not executed').hexdigest())

    def test_writes_checksum_and_manifest_sidecars_verbatim(self):
        with tempfile.TemporaryDirectory() as temp:
            result = self.execute(output=Path(temp) / 'out')
            output = Path(result['archive']).parent
            with zipfile.ZipFile(io.BytesIO(self.outer)) as candidate:
                for name in (self.archive_name + '.sha256', 'release-manifest.json.sha256'):
                    self.assertEqual((output / name).read_bytes(), candidate.read(name))

    def test_rejects_duplicate_identity_and_candidate(self):
        for artifacts in ([self.candidate], [self.identity, self.identity, self.candidate],
                          [self.identity, self.candidate, self.candidate]):
            with self.subTest(artifacts=artifacts):
                responses = {f'{downloader.API}/repos/o/r/actions/runs/{self.run_id}/artifacts?per_page=100':
                             json.dumps({'artifacts': artifacts}).encode(),
                             f'{downloader.API}/repos/o/r/actions/runs/{self.run_id}': json.dumps({
                                 'id': self.run_id, 'run_attempt': self.attempt, 'head_sha': self.source,
                                 'repository': {'full_name': 'o/r'}}).encode()}
                with patch.object(downloader, 'request', lambda url, *_args, **_kw: Response(responses[url])):
                    with self.assertRaises(downloader.DownloadError):
                        downloader.fetch_candidate('o/r', self.run_id, self.attempt, self.source,
                            self.target, self.name, '/tmp/unused-native-candidate', 'secret')

    def test_rejects_stale_identity_and_transport_digest(self):
        original = self.identity_zip
        self.identity_zip = zipped({'upload-identity.json': json.dumps({
            'artifact_id': '22', 'artifact_digest': self.candidate['digest'],
            'run_id': self.run_id, 'run_attempt': '1', 'target': self.target}).encode()})
        with self.assertRaises(downloader.DownloadError):
            self.execute()
        self.identity_zip = original
        bad = dict(self.candidate, digest='sha256:' + '0' * 64)
        with self.assertRaises(downloader.DownloadError):
            self.execute(candidate=bad)

    def test_rejects_run_source_and_target_binding_mismatches(self):
        for field, value in (('run_attempt', '1'), ('run_id', '999'),
                             ('target', 'aarch64-apple-darwin')):
            identity = {'artifact_id': '22', 'artifact_digest': self.candidate['digest'],
                        'run_id': self.run_id, 'run_attempt': self.attempt,
                        'target': self.target}
            identity[field] = value
            self.identity_zip = zipped({'upload-identity.json': json.dumps(identity).encode()})
            with self.subTest(field=field), self.assertRaises(downloader.DownloadError):
                self.execute()

    def test_rejects_manifest_binding_and_unsafe_zip(self):
        bad = zipped({'../escape': b'x'})
        old = self.candidate
        self.candidate = dict(old, digest=hashlib.sha256(bad).hexdigest())
        self.identity_zip = zipped({'upload-identity.json': json.dumps({
            'artifact_id': '22', 'artifact_digest': self.candidate['digest'],
            'run_id': self.run_id, 'run_attempt': self.attempt, 'target': self.target}).encode()})
        with self.assertRaises(downloader.DownloadError):
            self.execute(payload=bad)
        self.candidate = old

    def test_rejects_manifest_source_mismatch(self):
        # Rebuild a correctly checksummed transport whose manifest names a different source.
        files = {self.archive_name: b'archive', self.archive_name + '.sha256': b'0' * 64,
                 'release-manifest.json': json.dumps({'source_sha': 'b' * 40,
                    'run_id': self.run_id, 'run_attempt': self.attempt, 'target': self.target,
                    'version': self.version}).encode(),
                 'release-manifest.json.sha256': b'0' * 64}
        payload = zipped(files)
        self.candidate = dict(self.candidate, digest=hashlib.sha256(payload).hexdigest())
        self.identity_zip = zipped({'upload-identity.json': json.dumps({
            'artifact_id': '22', 'artifact_digest': self.candidate['digest'],
            'run_id': self.run_id, 'run_attempt': self.attempt, 'target': self.target}).encode()})
        with self.assertRaisesRegex(downloader.DownloadError, 'manifest source_sha'):
            self.execute(payload=payload)

    def test_redirect_does_not_forward_api_credentials(self):
        seen = []
        class Opener:
            def open(self, req, timeout=30):
                seen.append(req)
                if len(seen) == 1:
                    headers = email.message.Message()
                    headers['Location'] = 'https://downloads.example/file'
                    raise urllib.error.HTTPError(req.full_url, 302, 'redirect', headers, None)
                return Response(b'archive bytes')
        with patch.object(downloader.urllib.request, 'build_opener', return_value=Opener()):
            self.assertEqual(downloader.request('https://api.github.com/repos/o/r', 'secret').read(),
                             b'archive bytes')
        self.assertIn('Bearer secret', seen[0].get_header('Authorization'))
        self.assertIsNone(seen[1].get_header('Authorization'))


if __name__ == '__main__':
    unittest.main()
