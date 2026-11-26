"""Offline VSIX immutable-candidate handoff contract tests."""
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from scripts import download_native_candidate as transport
from scripts import download_vsix_candidate as downloader


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
        for name, data in files:
            archive.writestr(name, data)
    return stream.getvalue()


def unreachable(*_args, **_kwargs):
    """Fail if the downloader opens a response after a boundary rejection."""
    raise AssertionError('unexpected network request')


class VsixCandidateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        for lock in downloader.LOCKS:
            path = root / lock
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(('fixture ' + lock).encode())
        self.root = root
        self.version, self.run_id, self.attempt = '1.2.3', '101', '2'
        self.source, self.ref, self.event = 'a' * 40, 'refs/tags/v1.2.3', 'push'
        self.vsix = b'unchanged VSIX bytes'
        self.manifest = {'source_sha': self.source, 'original_ref': self.ref, 'event': self.event,
                         'run_id': self.run_id, 'run_attempt': self.attempt, 'version': self.version,
                         'vsix_sha256': hashlib.sha256(self.vsix).hexdigest(),
                         'lock_hashes': {name: hashlib.sha256((root / name).read_bytes()).hexdigest()
                                         for name in downloader.LOCKS},
                         'pnpm_version': downloader.PNPM_VERSION}
        self.payload = zipped([(f'magit-lsp-{self.version}.vsix', self.vsix),
                               ('vsix-manifest.json', json.dumps(self.manifest).encode())])
        self.candidate = {'id': 22, 'name': downloader.ARTIFACT,
                          'digest': hashlib.sha256(self.payload).hexdigest(), 'expired': False,
                          'workflow_run': {'id': self.run_id}}
        self.identity = {'id': 23, 'name': downloader.IDENTITY_ARTIFACT, 'expired': False}

    def publish(self, manifest):
        """Rebuild the candidate payload around a manifest variant."""
        data = zipped([(f'magit-lsp-{self.version}.vsix', self.vsix),
                       ('vsix-manifest.json', json.dumps(manifest).encode())])
        self.payload = data
        self.candidate = dict(self.candidate, digest=hashlib.sha256(data).hexdigest())

    def pair(self, attempt_value, artifact_id):
        """Build a candidate/identity pair produced by attempt_value."""
        manifest = dict(self.manifest, run_attempt=attempt_value)
        payload = zipped([(f'magit-lsp-{self.version}.vsix', self.vsix),
                          ('vsix-manifest.json', json.dumps(manifest).encode())])
        candidate = {'id': artifact_id, 'name': downloader.ARTIFACT,
                     'digest': hashlib.sha256(payload).hexdigest(), 'expired': False,
                     'workflow_run': {'id': self.run_id}}
        identity = {'id': artifact_id + 1, 'name': downloader.IDENTITY_ARTIFACT, 'expired': False}
        identity_zip = zipped([('vsix-upload-identity.json', json.dumps({
            'artifact_id': str(artifact_id), 'artifact_digest': candidate['digest'],
            'run_id': self.run_id, 'run_attempt': attempt_value}).encode())])
        return payload, candidate, identity, identity_zip

    def execute(self, *, listing=None, identity=None, payload=None, candidate=None,
                expected_locks=None, checkout=None, opener=None, producing_attempt=None):
        candidate = candidate or self.candidate
        producing_attempt = self.attempt if producing_attempt is None else producing_attempt
        identity = identity or {'artifact_id': str(candidate['id']), 'artifact_digest': candidate['digest'],
                                'run_id': self.run_id, 'run_attempt': producing_attempt}
        identity_zip = zipped([('vsix-upload-identity.json', json.dumps(identity).encode())])
        listing = listing if listing is not None else [self.identity, candidate]
        responses = {
            f'{transport.API}/repos/o/r/actions/runs/{self.run_id}': json.dumps({
                'id': self.run_id, 'run_attempt': self.attempt, 'head_sha': self.source,
                'repository': {'full_name': 'o/r'}}).encode(),
            f'{transport.API}/repos/o/r/actions/runs/{self.run_id}/artifacts?per_page=100': json.dumps({'artifacts': listing}).encode(),
            f'{transport.API}/repos/o/r/actions/artifacts/23/zip': identity_zip,
            f'{transport.API}/repos/o/r/actions/artifacts/22': json.dumps(candidate).encode(),
            f'{transport.API}/repos/o/r/actions/artifacts/22/zip': self.payload if payload is None else payload}
        def fake(url, *_args, **_kwargs):
            if url not in responses:
                raise AssertionError(url)
            return Response(responses[url])
        output = self.root / 'handoff'
        with patch.object(transport, 'json_get', lambda url, _token: json.loads(responses[url])):
            return downloader.fetch_candidate('o/r', self.run_id, self.attempt, self.source,
                self.ref, self.event, self.version, output, 'secret',
                checkout=self.root if checkout is None else checkout,
                expected_locks=expected_locks, opener=opener or fake)

    def test_handoff_binds_immutable_artifact_provenance_and_exact_bytes(self):
        result = self.execute()
        path = Path(result['vsix'])
        self.assertTrue(path.is_absolute())
        self.assertEqual(path.read_bytes(), self.vsix)
        self.assertEqual(result['vsix_sha256'], hashlib.sha256(self.vsix).hexdigest())
        self.assertEqual((result['artifact_id'], result['artifact_digest'], result['run_id'],
                          result['run_attempt'], result['source_sha']),
                         ('22', self.candidate['digest'], self.run_id, self.attempt, self.source))
        manifest = json.loads(Path(result['manifest']).read_text())
        self.assertEqual((manifest['original_ref'], manifest['event'], manifest['version']),
                         (self.ref, self.event, self.version))
        self.assertEqual(manifest['lock_hashes'], self.manifest['lock_hashes'])

    def test_binds_single_pair_produced_by_an_earlier_attempt(self):
        self.publish(dict(self.manifest, run_attempt='1'))
        result = self.execute(producing_attempt='1')
        self.assertEqual(result['run_attempt'], '1')
        handoff = json.loads((self.root / 'handoff' / 'vsix-candidate.json').read_text())
        self.assertEqual(handoff['run_attempt'], '1')

    def test_binds_matching_attempt_among_reruns_and_rejects_no_match(self):
        payload1, cand1, id1, zip1 = self.pair('1', 22)
        payload2, cand2, id2, zip2 = self.pair('2', 25)
        listing = [id1, id2, cand1, cand2]

        def run(attempt):
            responses = {
                f'{transport.API}/repos/o/r/actions/runs/{self.run_id}': json.dumps({
                    'id': self.run_id, 'run_attempt': '2', 'head_sha': self.source,
                    'repository': {'full_name': 'o/r'}}).encode(),
                f'{transport.API}/repos/o/r/actions/runs/{self.run_id}/artifacts?per_page=100':
                    json.dumps({'artifacts': listing}).encode(),
                f'{transport.API}/repos/o/r/actions/artifacts/{id1["id"]}/zip': zip1,
                f'{transport.API}/repos/o/r/actions/artifacts/{id2["id"]}/zip': zip2,
                f'{transport.API}/repos/o/r/actions/artifacts/22': json.dumps(cand1).encode(),
                f'{transport.API}/repos/o/r/actions/artifacts/22/zip': payload1,
                f'{transport.API}/repos/o/r/actions/artifacts/25': json.dumps(cand2).encode(),
                f'{transport.API}/repos/o/r/actions/artifacts/25/zip': payload2,
            }
            def fake(url, *_args, **_kwargs):
                if url not in responses:
                    raise AssertionError(url)
                return Response(responses[url])
            output = self.root / f'handoff-{attempt}'
            with patch.object(transport, 'json_get', lambda url, _token: json.loads(responses[url])):
                return downloader.fetch_candidate('o/r', self.run_id, attempt, self.source,
                    self.ref, self.event, self.version, output, 'secret',
                    checkout=self.root, expected_locks=None, opener=fake)

        self.assertEqual(run('2')['artifact_id'], '25')
        self.assertEqual(run('1')['artifact_id'], '22')
        with self.assertRaises(downloader.DownloadError):
            run('3')

    def test_rejects_missing_duplicate_expired_and_identity_mismatches(self):
        for listing in ([], [self.identity, self.identity, self.candidate],
                        [dict(self.identity, expired=True), self.candidate]):
            with self.subTest(listing=listing), self.assertRaises(downloader.DownloadError):
                self.execute(listing=listing)
        for field, value in [('artifact_id', '999'), ('artifact_digest', 'bad'),
                             ('run_id', '9')]:
            identity = {'artifact_id': '22', 'artifact_digest': self.candidate['digest'],
                        'run_id': self.run_id, 'run_attempt': self.attempt}
            identity[field] = value
            with self.subTest(field=field), self.assertRaises(downloader.DownloadError):
                self.execute(identity=identity)

    def test_rejects_bad_transport_manifest_bindings_hash_and_zip_members(self):
        with self.assertRaises(downloader.DownloadError):
            self.execute(payload=b'not the recorded digest')
        for files in ([('magit-lsp-1.2.3.vsix', self.vsix), ('vsix-manifest.json', b'{}'), ('extra', b'x')],
                      [('../escape', b'x'), ('magit-lsp-1.2.3.vsix', self.vsix), ('vsix-manifest.json', b'{}')],
                      [('magit-lsp-1.2.3.vsix', self.vsix), ('magit-lsp-1.2.3.vsix', self.vsix), ('vsix-manifest.json', b'{}')],
                      [('magit-lsp-1.2.3.vsix', self.vsix)]):
            data = zipped(files)
            candidate = dict(self.candidate, digest=hashlib.sha256(data).hexdigest())
            with self.subTest(files=files), self.assertRaises(downloader.DownloadError):
                self.execute(payload=data, candidate=candidate)
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, 'w') as archive:
            link = zipfile.ZipInfo('magit-lsp-1.2.3.vsix')
            link.create_system = 3
            link.external_attr = (0o120777 << 16)
            archive.writestr(link, 'target')
            archive.writestr('vsix-manifest.json', json.dumps(self.manifest))
        link_data = stream.getvalue()
        with self.assertRaises(downloader.DownloadError):
            self.execute(payload=link_data, candidate=dict(self.candidate,
                digest=hashlib.sha256(link_data).hexdigest()))
        for field, value in [('source_sha', 'b' * 40), ('original_ref', 'refs/heads/wrong'),
                             ('event', 'workflow_dispatch'), ('run_id', '9'), ('run_attempt', '1'),
                             ('version', '9.9.9'), ('pnpm_version', 'wrong'),
                             ('lock_hashes', {})]:
            manifest = dict(self.manifest, **{field: value})
            data = zipped([(f'magit-lsp-{self.version}.vsix', self.vsix),
                           ('vsix-manifest.json', json.dumps(manifest).encode())])
            candidate = dict(self.candidate, digest=hashlib.sha256(data).hexdigest())
            with self.subTest(field=field), self.assertRaises(downloader.DownloadError):
                self.execute(payload=data, candidate=candidate)
        manifest = dict(self.manifest, vsix_sha256='0' * 64)
        data = zipped([(f'magit-lsp-{self.version}.vsix', self.vsix),
                       ('vsix-manifest.json', json.dumps(manifest).encode())])
        with self.assertRaises(downloader.DownloadError):
            self.execute(payload=data, candidate=dict(self.candidate, digest=hashlib.sha256(data).hexdigest()))

    def test_explicit_expected_locks_accept_release_source_and_reject_mismatch(self):
        release = {name: hashlib.sha256(('release source ' + name).encode()).hexdigest()
                   for name in downloader.LOCKS}
        self.publish(dict(self.manifest, lock_hashes=release))
        result = self.execute(expected_locks=release, checkout=self.root / 'absent-checkout')
        manifest = json.loads(Path(result['manifest']).read_text())
        self.assertEqual(manifest['lock_hashes'], release)
        wrong = dict(release, **{downloader.LOCKS[0]: '0' * 64})
        for label, expectations in (('checkout locks', self.manifest['lock_hashes']),
                                    ('one wrong digest', wrong)):
            with self.subTest(label=label), self.assertRaises(downloader.DownloadError):
                self.execute(expected_locks=expectations, checkout=self.root / 'absent-checkout')

    def test_explicit_expected_locks_validate_digest_set_at_boundary(self):
        release = {name: 'a' * 64 for name in downloader.LOCKS}
        self.publish(dict(self.manifest, lock_hashes=release))
        for label, expectations in (
                ('missing lock', {name: release[name] for name in downloader.LOCKS[1:]}),
                ('unknown lock', dict(release, **{'Cargo.toml': 'a' * 64})),
                ('uppercase digest', dict(release, **{downloader.LOCKS[0]: 'A' * 64})),
                ('short digest', dict(release, **{downloader.LOCKS[0]: 'a' * 63})),
                ('non-string digest', dict(release, **{downloader.LOCKS[0]: 7})),
                ('wrong container', [release[name] for name in downloader.LOCKS])):
            with self.subTest(label=label), self.assertRaises(downloader.DownloadError):
                self.execute(expected_locks=expectations, opener=unreachable)

    def test_omitted_expectations_keep_checkout_lock_and_integrity_guards(self):
        self.execute()
        release = {name: hashlib.sha256(('release source ' + name).encode()).hexdigest()
                   for name in downloader.LOCKS}
        self.publish(dict(self.manifest, lock_hashes=release))
        with self.assertRaises(downloader.DownloadError):
            self.execute()
        for name in downloader.LOCKS:
            self.publish(dict(self.manifest, lock_hashes=dict(self.manifest['lock_hashes'],
                                                               **{name: '0' * 64})))
            with self.subTest(lock=name), self.assertRaises(downloader.DownloadError):
                self.execute()


if __name__ == '__main__':
    unittest.main()
