#!/usr/bin/env python3
"""Fetch and validate an immutable VSIX candidate without executing it."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tempfile

from scripts import download_native_candidate as transport

ARTIFACT = 'vscode-vsix-candidate'
IDENTITY_ARTIFACT = 'vscode-vsix-candidate-identity'
LOCKS = ('Cargo.lock', 'flake.lock', 'clients/vscode/pnpm-lock.yaml')
PNPM_VERSION = '10.13.1'


DownloadError = transport.DownloadError


def expected_lock_digests(locks):
    """Validate explicit expected lock digests for every tracked lock file.

    locks must map exactly LOCKS to lowercase sha256 hex digests; the result is
    compared to the manifest verbatim, so no digest is normalized or filled in.
    """
    if not isinstance(locks, dict) or set(locks) != set(LOCKS):
        raise DownloadError(f'expected locks must name exactly {", ".join(LOCKS)}')
    if any(not isinstance(value, str) or not re.fullmatch(r'[0-9a-f]{64}', value)
           for value in locks.values()):
        raise DownloadError('invalid expected lock digest')
    return dict(locks)


def fetch_candidate(repository, run_id, attempt, source_sha, original_ref, event, version,
                    output, token, *, checkout='.', expected_locks=None, opener=transport.request):
    """Validate and unpack the exact run-bound VSIX artifact into output.

    expected_locks, when supplied, is the digest map of the bound release source
    and replaces the lockfiles read from the publisher checkout.
    """
    if not re.fullmatch(r'[0-9a-f]{40}', str(source_sha)):
        raise DownloadError('invalid source SHA')
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?', str(version)):
        raise DownloadError('invalid VSIX version')
    if expected_locks is not None:
        expected_locks = expected_lock_digests(expected_locks)
    owner, sep, repo = repository.partition('/')
    if not sep or not owner or not repo or not all((run_id, attempt, original_ref, event)):
        raise DownloadError('invalid repository or missing run binding')
    base = f'{transport.API}/repos/{owner}/{repo}/actions/runs/{run_id}'
    run = transport.json_get(base, token)
    if (str(run.get('id')) != str(run_id)
            or run.get('head_sha') != source_sha
            or run.get('repository', {}).get('full_name', repository).lower() != repository.lower()):
        raise DownloadError('requested run/repository/source binding mismatch')
    if str(run.get('run_attempt')) != str(attempt):
        print(f'VSIX candidate download: run attempt {run.get("run_attempt")!r} differs from '
              f'requested attempt {attempt!r}; binding by artifact identity', file=sys.stderr)
    listing = transport.json_get(base + '/artifacts?per_page=100', token).get('artifacts', [])
    identity_items = [item for item in listing if item.get('name') == IDENTITY_ARTIFACT and not item.get('expired')]
    candidates = [item for item in listing if item.get('name') == ARTIFACT and not item.get('expired')]
    if not identity_items or not candidates:
        raise DownloadError('missing VSIX identity/candidate')
    if (len({str(item.get('id')) for item in identity_items}) != len(identity_items)
            or len({str(item.get('id')) for item in candidates}) != len(candidates)):
        raise DownloadError('duplicate VSIX identity/candidate')
    # An identity records the attempt that produced its candidate; a partial
    # re-run leaves that attempt lower than the consumer's requested attempt.
    pairs = []
    for identity_artifact in identity_items:
        with opener(f'{transport.API}/repos/{owner}/{repo}/actions/artifacts/{identity_artifact["id"]}/zip', token) as response:
            identity_files = transport.exact_zip(response.read(), ['vsix-upload-identity.json'])
        identity = json.loads(identity_files['vsix-upload-identity.json'])
        matches = [item for item in candidates if str(item.get('id')) == str(identity.get('artifact_id'))]
        if not matches:
            continue
        candidate = matches[0]
        if identity != {'artifact_id': str(candidate['id']), 'artifact_digest': candidate.get('digest'),
                        'run_id': str(run_id), 'run_attempt': str(identity.get('run_attempt'))}:
            raise DownloadError('upload identity does not bind candidate/run')
        if str(candidate.get('workflow_run', {}).get('id', run_id)) != str(run_id):
            raise DownloadError('candidate belongs to another run')
        pairs.append((candidate, identity))
    matching = [pair for pair in pairs if str(pair[1].get('run_attempt')) == str(attempt)]
    if len(pairs) == 1:
        candidate, identity = pairs[0]
    elif len(matching) == 1:
        candidate, identity = matching[0]
    else:
        raise DownloadError('no unique VSIX identity/candidate pair for the requested attempt')
    producing_attempt = str(identity['run_attempt'])
    metadata = transport.json_get(f'{transport.API}/repos/{owner}/{repo}/actions/artifacts/{candidate["id"]}', token)
    if (str(metadata.get('id')) != str(candidate['id']) or metadata.get('digest') != candidate.get('digest')
            or metadata.get('expired') or metadata.get('name') != ARTIFACT):
        raise DownloadError('candidate metadata identity/digest mismatch')
    with opener(f'{transport.API}/repos/{owner}/{repo}/actions/artifacts/{candidate["id"]}/zip', token) as response:
        payload = response.read()
    if transport.digest(payload) != str(candidate.get('digest', '')).removeprefix('sha256:'):
        raise DownloadError('downloaded artifact transport digest mismatch')
    try:
        members = transport.exact_zip(payload, {f'magit-lsp-{version}.vsix', 'vsix-manifest.json'})
        manifest_bytes = members['vsix-manifest.json']
        manifest = json.loads(manifest_bytes)
        expected_locks = expected_locks if expected_locks is not None else {
            name: hashlib.sha256((Path(checkout) / name).read_bytes()).hexdigest() for name in LOCKS}
        expected = {'source_sha': source_sha, 'original_ref': original_ref, 'event': event,
                    'run_id': str(run_id), 'run_attempt': producing_attempt, 'version': str(version),
                    'lock_hashes': expected_locks, 'pnpm_version': PNPM_VERSION}
        if any(manifest.get(key) != value for key, value in expected.items()):
            raise DownloadError('VSIX manifest provenance does not match requested checkout/run')
        vsix = members[f'magit-lsp-{version}.vsix']
        if transport.digest(vsix) != manifest.get('vsix_sha256'):
            raise DownloadError('VSIX digest differs from manifest')
        destination = Path(output).resolve()
        destination.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='vsix-candidate-', dir=destination) as temp:
            staged = Path(temp)
            (staged / f'magit-lsp-{version}.vsix').write_bytes(vsix)
            (staged / 'vsix-manifest.json').write_bytes(manifest_bytes)
            vsix_path, manifest_path = destination / f'magit-lsp-{version}.vsix', destination / 'vsix-manifest.json'
            os.replace(staged / vsix_path.name, vsix_path)
            os.replace(staged / manifest_path.name, manifest_path)
        handoff = {'artifact_id': str(candidate['id']), 'artifact_digest': candidate['digest'],
                   'run_id': str(run_id), 'run_attempt': producing_attempt, 'source_sha': source_sha,
                   'vsix': str(vsix_path), 'manifest': str(manifest_path),
                   'vsix_sha256': transport.digest(vsix)}
        (destination / 'vsix-candidate.json').write_text(json.dumps(handoff, sort_keys=True) + '\n')
        return handoff
    except (KeyError, ValueError, OSError) as error:
        raise DownloadError(f'invalid VSIX candidate: {error}') from error


def main(argv=None):
    parser = argparse.ArgumentParser()
    for name in ('repository', 'run-id', 'attempt', 'source-sha', 'ref', 'event', 'version', 'output'):
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--checkout', default='.')
    args = parser.parse_args(argv)
    token = os.environ.get('GITHUB_TOKEN')
    if not token:
        parser.error('GITHUB_TOKEN environment variable is required')
    try:
        result = fetch_candidate(args.repository, args.run_id, args.attempt, args.source_sha,
                                 args.ref, args.event, args.version, args.output, token, checkout=args.checkout)
    except (DownloadError, KeyError, ValueError, OSError) as error:
        print(f'VSIX candidate download failed: {error}', file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
