#!/usr/bin/env python3
"""Fetch and validate one immutable native candidate without executing it."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import zipfile

from scripts import check_native_artifact as native

API = 'https://api.github.com'
TARGETS = ('x86_64-unknown-linux-gnu', 'aarch64-apple-darwin')


class DownloadError(Exception):
    """An API response or candidate artifact violated the handoff contract."""


class NoRedirect(urllib.request.HTTPRedirectHandler):
    """Keep redirects explicit so API credentials never cross origins."""
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def request(url, token, *, api=True):
    """GET one URL, attaching credentials only to the fixed GitHub API origin."""
    headers = {'Accept': 'application/vnd.github+json'} if api else {}
    if api:
        if urllib.parse.urlsplit(url).scheme != 'https' or urllib.parse.urlsplit(url).netloc != 'api.github.com':
            raise DownloadError('refusing non-API URL for authenticated request')
        headers['Authorization'] = f'Bearer {token}'
    opener = urllib.request.build_opener(NoRedirect)
    req = urllib.request.Request(url, headers=headers)
    try:
        return opener.open(req, timeout=30)
    except urllib.error.HTTPError as error:
        if error.code not in (301, 302, 303, 307, 308):
            raise DownloadError(f'GitHub API request failed with HTTP {error.code}') from error
        location = error.headers.get('Location')
        parsed = urllib.parse.urlsplit(urllib.parse.urljoin(url, location or ''))
        if parsed.scheme != 'https' or not parsed.netloc:
            error.close()
            raise DownloadError('artifact redirect must use HTTPS') from error
        error.close()
        return request(parsed.geturl(), token, api=(parsed.netloc == 'api.github.com'))


def json_get(url, token):
    with request(url, token) as response:
        return json.loads(response.read())


def digest(data):
    return hashlib.sha256(data).hexdigest()


def exact_zip(data, expected=None):
    """Read a zip containing only unique, regular, safe expected file names."""
    try:
        archive = zipfile.ZipFile(io.BytesIO(data))
        infos = archive.infolist()
        names = [item.filename for item in infos]
        for item in infos:
            path = PurePosixPath(item.filename)
            mode = item.external_attr >> 16
            if (path.is_absolute() or '..' in path.parts or '\\' in item.filename
                    or item.is_dir() or (mode and (mode & 0o170000) not in (0, 0o100000))):
                raise DownloadError(f'unsafe zip member {item.filename!r}')
        if len(names) != len(set(names)) or (expected is not None and set(names) != set(expected)):
            raise DownloadError('candidate zip member set is not exact')
        if expected is None and (len(names) != 4 or
                sorted(PurePosixPath(name).name for name in names)[-2:] !=
                ['release-manifest.json', 'release-manifest.json.sha256']):
            raise DownloadError('candidate zip member set is not exact')
        return {name: archive.read(name) for name in names}
    except (OSError, zipfile.BadZipFile) as error:
        raise DownloadError('invalid artifact zip') from error


def fetch_candidate(repository, run_id, attempt, source_sha, target, artifact_name, output,
                    token, opener=request):
    """Fetch identity and candidate by immutable ID, validate bindings, emit handoff JSON."""
    if target not in TARGETS or not isinstance(source_sha, str) or not re.fullmatch(r'[0-9a-f]{40}', source_sha):
        raise DownloadError('invalid source SHA or unsupported target')
    owner, sep, repo = repository.partition('/')
    if not sep or not owner or not repo:
        raise DownloadError('repository must be OWNER/REPO')
    base = f'{API}/repos/{owner}/{repo}/actions/runs/{run_id}/artifacts?per_page=100'
    run = json_get(f'{API}/repos/{owner}/{repo}/actions/runs/{run_id}', token)
    if (str(run.get('id')) != str(run_id)
            or run.get('head_sha') != source_sha
            or run.get('repository', {}).get('full_name', repository).lower() != repository.lower()):
        raise DownloadError('requested run/repository/source binding mismatch')
    if str(run.get('run_attempt')) != str(attempt):
        print(f'native candidate download: run attempt {run.get("run_attempt")!r} differs from '
              f'requested attempt {attempt!r}; binding by artifact identity', file=sys.stderr)
    artifacts = json_get(base, token)
    entries = artifacts.get('artifacts', [])
    identities = [a for a in entries if a.get('name') == f'{artifact_name}-identity'
                  and not a.get('expired')]
    candidates = [a for a in entries if a.get('name') == artifact_name and not a.get('expired')]
    if not identities or not candidates:
        raise DownloadError('missing identity/candidate artifact')
    if (len({str(a.get('id')) for a in identities}) != len(identities)
            or len({str(a.get('id')) for a in candidates}) != len(candidates)):
        raise DownloadError('duplicate identity/candidate artifact')
    # An identity records the attempt that produced its candidate; a partial
    # re-run leaves that attempt lower than the consumer's requested attempt.
    pairs = []
    for identity_artifact in identities:
        with opener(f'{API}/repos/{owner}/{repo}/actions/artifacts/{identity_artifact["id"]}/zip', token) as response:
            identity_members = exact_zip(response.read(), ['upload-identity.json'])
        identity = json.loads(identity_members['upload-identity.json'])
        matches = [a for a in candidates if str(a.get('id')) == str(identity.get('artifact_id'))]
        if not matches:
            continue
        candidate = matches[0]
        expected = {'artifact_id': str(candidate['id']), 'artifact_digest': candidate.get('digest'),
                    'run_id': str(run_id), 'run_attempt': str(identity.get('run_attempt')),
                    'target': target}
        if identity != expected or candidate.get('digest') != identity.get('artifact_digest'):
            raise DownloadError('upload identity does not bind the requested artifact/run/target')
        if str(candidate.get('workflow_run', {}).get('id', run_id)) != str(run_id):
            raise DownloadError('candidate belongs to another run')
        pairs.append((candidate, identity))
    matching = [pair for pair in pairs if str(pair[1].get('run_attempt')) == str(attempt)]
    if len(pairs) == 1:
        candidate, identity = pairs[0]
    elif len(matching) == 1:
        candidate, identity = matching[0]
    else:
        raise DownloadError('no unique identity/candidate pair for the requested attempt')
    producing_attempt = str(identity['run_attempt'])
    metadata = json_get(f'{API}/repos/{owner}/{repo}/actions/artifacts/{candidate["id"]}', token)
    if (str(metadata.get('id')) != str(candidate['id'])
            or metadata.get('digest') != candidate.get('digest')
            or metadata.get('expired') or metadata.get('name') != artifact_name):
        raise DownloadError('candidate metadata identity/digest mismatch')
    with opener(f'{API}/repos/{owner}/{repo}/actions/artifacts/{candidate["id"]}/zip', token) as response:
        payload = response.read()
    expected_transport = str(candidate.get('digest', '')).removeprefix('sha256:')
    if digest(payload) != expected_transport:
        raise DownloadError('downloaded artifact transport digest mismatch')
    with tempfile.TemporaryDirectory(prefix='native-candidate-') as temp:
        stage = Path(temp)
        files = exact_zip(payload)
        manifest = json.loads(files['release-manifest.json'])
        for key, value in {'source_sha': source_sha, 'run_id': str(run_id),
                           'run_attempt': producing_attempt, 'target': target}.items():
            if str(manifest.get(key)) != value:
                raise DownloadError(f'manifest {key} does not match request')
        archive_name = f'magit-lsp-v{manifest["version"]}-{target}.tar.gz'
        required_names = {archive_name, archive_name + '.sha256',
                          'release-manifest.json', 'release-manifest.json.sha256'}
        if {PurePosixPath(name).name for name in files} != required_names:
            raise DownloadError('candidate zip members do not match manifest version and target')
        # Artifact member names are producer-defined; require exact files by basename.
        archive_data = next((v for k, v in files.items() if PurePosixPath(k).name == archive_name), None)
        checksum_data = next((v for k, v in files.items()
                              if PurePosixPath(k).name == archive_name + '.sha256'), None)
        if archive_data is None or checksum_data is None:
            raise DownloadError('candidate archive/checksum missing')
        if digest(archive_data) != manifest.get('archive_sha256'):
            raise DownloadError('archive digest differs from manifest')
        if digest(files['release-manifest.json']) != files['release-manifest.json.sha256'].decode().split()[0]:
            raise DownloadError('manifest sidecar digest mismatch')
        if checksum_data.decode().split()[0] != digest(archive_data):
            raise DownloadError('archive checksum sidecar mismatch')
        archive_path = stage / archive_name
        archive_path.write_bytes(archive_data)
        binary = native.safe_extract(archive_path, stage / 'extracted', archive_name.removesuffix('.tar.gz'))
        if native.digest(binary) != manifest.get('executable_sha256'):
            raise DownloadError('extracted executable digest differs from manifest')
        destination = Path(output).resolve()
        destination.mkdir(parents=True, exist_ok=True)
        for name, content in ((archive_name, archive_data),
                              (archive_name + '.sha256', checksum_data),
                              ('release-manifest.json', files['release-manifest.json']),
                              ('release-manifest.json.sha256', files['release-manifest.json.sha256'])):
            (destination / name).write_bytes(content)
        handoff = {'artifact_id': str(candidate['id']), 'artifact_digest': candidate['digest'],
                   'run_id': str(run_id), 'run_attempt': producing_attempt, 'source_sha': source_sha,
                   'target': target, 'archive': str(destination / archive_name),
                   'manifest': str(destination / 'release-manifest.json'),
                   'archive_sha256': digest(archive_data), 'executable_sha256': manifest['executable_sha256']}
        (destination / 'native-candidate.json').write_text(json.dumps(handoff, sort_keys=True) + '\n')
        return handoff


def main(argv=None):
    parser = argparse.ArgumentParser()
    for name in ('repository', 'run-id', 'attempt', 'source-sha', 'target', 'artifact-name', 'output'):
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args(argv)
    token = os.environ.get('GITHUB_TOKEN')
    if not token:
        parser.error('GITHUB_TOKEN environment variable is required')
    try:
        result = fetch_candidate(args.repository, args.run_id, args.attempt, args.source_sha,
                                 args.target, args.artifact_name, args.output, token)
    except (DownloadError, KeyError, ValueError, OSError) as error:
        print(f'native candidate download failed: {error}', file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
