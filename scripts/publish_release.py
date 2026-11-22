#!/usr/bin/env python3
"""Publish one already-validated release handoff without building anything.

The protected publisher downloads ``publication-handoff.json`` and its exact
asset bytes from the read-only validation job. This module revalidates that
handoff against the trusted ``workflow_run`` event, then mutates destinations in
one fixed order: the GitHub release with the exact accepted assets, the VS Code
Marketplace through ``vsce publish --oidc`` and Open VSX through
``ovsx publish --trusted-publishing``. Both registry lanes address the identical
accepted VSIX bytes; nothing is rebuilt, repackaged, installed from a personal
access token or retried. A failure after any destination reports the completed
and pending destinations and exits nonzero, never as atomic success.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

# These mirror the producer/checker contract; the release-automation self-tests
# assert equality so the two definitions cannot drift silently.
BUILD_WORKFLOW_PATH = '.github/workflows/release.yml'
BUILD_WORKFLOW_NAME = 'Release artifact build (no publication)'
TARGET = 'x86_64-unknown-linux-gnu'
VERSION_PATTERN = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)')
HANDOFF_FIELDS = frozenset({'source_sha', 'version', 'original_ref', 'event',
                            'run_id', 'run_attempt', 'target', 'native', 'vsix'})
NATIVE_FIELDS = frozenset({'artifact_id', 'artifact_digest', 'archive', 'checksum',
                           'manifest', 'manifest_checksum', 'archive_sha256',
                           'manifest_sha256', 'executable_sha256'})
VSIX_FIELDS = frozenset({'artifact_id', 'artifact_digest', 'run_id', 'run_attempt',
                         'source_sha', 'vsix', 'manifest', 'vsix_sha256'})
MANIFEST_FIELDS = frozenset({
    'source_sha', 'original_ref', 'event', 'run_id', 'run_attempt',
    'version', 'lock_hashes', 'target', 'toolchain', 'archive_sha256',
    'executable_sha256',
})
VSIX_MANIFEST_FIELDS = frozenset({'source_sha', 'original_ref', 'event', 'run_id',
                                  'run_attempt', 'version', 'vsix_sha256',
                                  'lock_hashes', 'pnpm_version'})
OIDC_ENV = ('ACTIONS_ID_TOKEN_REQUEST_URL', 'ACTIONS_ID_TOKEN_REQUEST_TOKEN')
PAT_ENV = ('VSCE_PAT', 'OVSX_PAT', 'VSCE_TOKEN', 'OVSX_TOKEN')
DESTINATIONS = ('github-release', 'marketplace', 'openvsx')
VSCE = Path('clients/vscode/node_modules/.bin/vsce')
OVSX = Path('clients/vscode/node_modules/.bin/ovsx')
HEX64 = re.compile(r'[0-9a-f]{64}')
SHA256_DIGEST = re.compile(r'sha256:[0-9a-f]{64}')
SIDECAR = re.compile(r'([0-9a-f]{64})  (\S+)\n')


class PublishError(Exception):
    """A handoff, trusted context or external publication step failed closed."""


def fail(message):
    raise PublishError(message)


def full_name(value):
    """Return ``full_name`` from an API repository object, or ``None``."""
    return value.get('full_name') if isinstance(value, dict) else None


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def parse_json(document, label):
    try:
        return json.loads(document)
    except (TypeError, ValueError) as error:
        fail(f'{label}: invalid JSON ({error})')


def load_handoff(path):
    """Return ``(document, handoff directory)`` after path-safety validation."""
    path = Path(path)
    if path.is_symlink():
        fail('publication handoff must not be a symbolic link')
    if not path.is_file():
        fail(f'publication handoff {path} is missing')
    document = parse_json(path.read_bytes(), 'publication handoff')
    if not isinstance(document, dict):
        fail('publication handoff: expected a JSON object')
    if set(document) != HANDOFF_FIELDS:
        missing = sorted(HANDOFF_FIELDS - set(document))
        unexpected = sorted(set(document) - HANDOFF_FIELDS)
        fail(f'publication handoff fields differ: missing {missing}, '
             f'unexpected {unexpected}')
    return document, path.resolve().parent


def validate_handoff_binding(document):
    """Validate the handoff's fixed schema, names and run/source identity."""
    source = document['source_sha']
    if not isinstance(source, str) or not re.fullmatch(r'[0-9a-f]{40}', source):
        fail('publication handoff source_sha is not a full commit SHA')
    version = document['version']
    if not isinstance(version, str) or not VERSION_PATTERN.fullmatch(version):
        fail(f'publication handoff version {version!r} is not a stable X.Y.Z')
    if document['original_ref'] != f'refs/tags/v{version}':
        fail(f'publication handoff original_ref {document["original_ref"]!r} is not '
             f'the version tag v{version}')
    if document['event'] != 'push':
        fail(f'publication handoff event is {document["event"]!r}, expected push')
    if document['target'] != TARGET:
        fail(f'publication handoff target is {document["target"]!r}, '
             f'expected {TARGET!r}')
    for field in ('run_id', 'run_attempt'):
        if not isinstance(document[field], str) or not re.fullmatch(r'[0-9]+', document[field]):
            fail(f'publication handoff {field} {document[field]!r} is not a run number')
    native = document['native']
    if not isinstance(native, dict) or set(native) != NATIVE_FIELDS:
        fail('publication handoff native binding has the wrong fields')
    if not SHA256_DIGEST.fullmatch(str(native['artifact_digest'])):
        fail('publication handoff native artifact digest is unsupported')
    for field in ('archive_sha256', 'manifest_sha256', 'executable_sha256'):
        if not HEX64.fullmatch(str(native[field])):
            fail(f'publication handoff native {field} is not a SHA-256')
    archive = f'magit-lsp-v{version}-{TARGET}.tar.gz'
    names = {
        'archive': archive,
        'checksum': archive + '.sha256',
        'manifest': 'release-manifest.json',
        'manifest_checksum': 'release-manifest.json.sha256',
    }
    for field, expected in names.items():
        if native[field] != expected:
            fail(f'publication handoff native {field} is {native[field]!r}, '
                 f'expected {expected!r}')
    vsix = document['vsix']
    if not isinstance(vsix, dict) or set(vsix) != VSIX_FIELDS:
        fail('publication handoff VSIX binding has the wrong fields')
    if not SHA256_DIGEST.fullmatch(str(vsix['artifact_digest'])):
        fail('publication handoff VSIX artifact digest is unsupported')
    if not HEX64.fullmatch(str(vsix['vsix_sha256'])):
        fail('publication handoff VSIX hash is not a SHA-256')
    for field, expected in (('run_id', document['run_id']),
                            ('run_attempt', document['run_attempt']),
                            ('source_sha', document['source_sha'])):
        if vsix[field] != expected:
            fail(f'publication handoff VSIX {field} is {vsix[field]!r}, '
                 f'expected {expected!r}')
    if vsix['vsix'] != f'magit-lsp-{version}.vsix':
        fail(f'publication handoff VSIX name is {vsix["vsix"]!r}, expected '
             f'magit-lsp-{version}.vsix')
    if vsix['manifest'] != 'vsix-manifest.json':
        fail(f'publication handoff VSIX manifest is {vsix["manifest"]!r}, '
             f'expected vsix-manifest.json')


def trusted_context(document, environment, event):
    """Revalidate the handoff against the trusted workflow_run event and OIDC.

    The event is trusted orchestration data, not artifact content: only a
    successful push build of the release workflow in this repository, at the
    handoff's exact run identity and version tag, may pass. Trusted publishing
    requires the Actions OIDC environment; a stored PAT is explicitly refused so
    a credential fallback can never turn a missing OIDC request into success.
    """
    if environment.get('GITHUB_ACTIONS') != 'true':
        fail('publication requires the GitHub Actions context')
    if environment.get('GITHUB_EVENT_NAME') != 'workflow_run':
        fail('publication requires a workflow_run event')
    repository = environment.get('GITHUB_REPOSITORY', '')
    if not re.fullmatch(r'[\w.\-]+/[\w.\-]+', repository):
        fail(f'GITHUB_REPOSITORY is not a repository slug: {repository!r}')
    for name in PAT_ENV:
        if environment.get(name):
            fail(f'PAT fallback {name} is forbidden; OIDC trusted publishing is required')
    for name in OIDC_ENV:
        if not environment.get(name):
            fail(f'OIDC environment {name} is required; no PAT fallback is allowed')
    if not isinstance(event, dict) or event.get('action') != 'completed':
        fail('publication event must be workflow_run completed')
    run = event.get('workflow_run')
    if not isinstance(run, dict):
        fail('publication event has no workflow_run')
    if full_name(run.get('repository')) != repository:
        fail('publication event run belongs to another repository')
    if run.get('path') != BUILD_WORKFLOW_PATH or run.get('name') != BUILD_WORKFLOW_NAME:
        fail('publication event run is not the release artifact build')
    if run.get('event') != 'push' or run.get('conclusion') != 'success':
        fail('publication event run must be a successful push build')
    if str(run.get('id')) != document['run_id'] or \
            str(run.get('run_attempt')) != document['run_attempt']:
        fail('publication handoff run identity differs from the trusted event')
    tag = f'v{document["version"]}'
    if run.get('head_branch') != tag:
        fail(f'publication event tag is {run.get("head_branch")!r}, expected {tag!r}')
    if run.get('head_sha') != document['source_sha']:
        fail('publication handoff source differs from the trusted tag commit')
    return repository


def asset_path(root, name, label):
    """Resolve one handoff-relative regular file, refusing escapes and symlinks."""
    if not isinstance(name, str) or not name or '\\' in name:
        fail(f'publication {label} path is invalid')
    relative = Path(name)
    if relative.is_absolute() or '..' in relative.parts or len(relative.parts) != 1:
        fail(f'publication {label} path {name!r} escapes the handoff directory')
    candidate = root / relative.name
    if candidate.is_symlink():
        fail(f'publication {label} {name!r} must not be a symbolic link')
    try:
        resolved = candidate.resolve(strict=True)
    except OSError:
        fail(f'publication {label} {name!r} is missing')
    if resolved.parent != root:
        fail(f'publication {label} {name!r} escapes the handoff directory')
    if not resolved.is_file():
        fail(f'publication {label} {name!r} is not a regular file')
    return resolved


def read_sidecar(path, expected_name, label):
    """Return the digest recorded by a ``<sha256>  <name>`` sidecar file."""
    try:
        text = path.read_text(encoding='utf-8')
    except (OSError, UnicodeDecodeError) as error:
        fail(f'publication {label} is unreadable: {error}')
    match = SIDECAR.fullmatch(text)
    if not match or match.group(2) != expected_name:
        fail(f'publication {label} must record {expected_name!r}')
    return match.group(1)


def validate_manifest(manifest, document, native):
    """Bind the persisted native manifest to the handoff and exact names."""
    if not isinstance(manifest, dict) or set(manifest) != MANIFEST_FIELDS:
        fail('publication release manifest fields differ from the producer contract')
    if manifest['source_sha'] != document['source_sha'] or \
            manifest['version'] != document['version'] or \
            manifest['original_ref'] != document['original_ref'] or \
            manifest['event'] != document['event'] or \
            manifest['run_id'] != document['run_id'] or \
            manifest['run_attempt'] != document['run_attempt'] or \
            manifest['target'] != document['target']:
        fail('publication release manifest provenance differs from the handoff')
    if manifest['archive_sha256'] != native['archive_sha256'] or \
            manifest['executable_sha256'] != native['executable_sha256']:
        fail('publication release manifest hashes differ from the handoff')


def validate_vsix_manifest(manifest, document, vsix):
    """Bind the persisted VSIX manifest to the handoff and exact VSIX hash."""
    if not isinstance(manifest, dict) or set(manifest) != VSIX_MANIFEST_FIELDS:
        fail('publication VSIX manifest fields differ from the producer contract')
    for field in ('source_sha', 'original_ref', 'event', 'run_id', 'run_attempt',
                  'version'):
        if manifest[field] != document[field]:
            fail(f'publication VSIX manifest {field} differs from the handoff')
    if manifest['vsix_sha256'] != vsix['vsix_sha256']:
        fail('publication VSIX manifest hash differs from the handoff')


def load_assets(document, root):
    """Verify every accepted byte before any privileged mutation.

    The archive, checksum, manifest, manifest sidecar and VSIX all resolve
    inside the handoff directory, are regular non-symlink files and reproduce
    the digests recorded by the handoff and their own manifests. Any changed
    byte, escaped path or mismatch fails before a destination is touched.
    """
    native, vsix = document['native'], document['vsix']
    archive = asset_path(root, native['archive'], 'archive')
    checksum = asset_path(root, native['checksum'], 'checksum')
    manifest_path = asset_path(root, native['manifest'], 'manifest')
    manifest_checksum = asset_path(root, native['manifest_checksum'],
                                   'manifest checksum')
    vsix_path = asset_path(root, vsix['vsix'], 'VSIX')
    vsix_manifest_path = asset_path(root, vsix['manifest'], 'VSIX manifest')

    archive_digest = digest_bytes(archive.read_bytes())
    if archive_digest != native['archive_sha256']:
        fail('publication archive bytes differ from the handoff')
    if read_sidecar(checksum, native['archive'], 'archive checksum') != archive_digest:
        fail('publication archive checksum differs from the accepted archive')
    manifest_bytes = manifest_path.read_bytes()
    manifest_digest = digest_bytes(manifest_bytes)
    if manifest_digest != native['manifest_sha256']:
        fail('publication release manifest bytes differ from the handoff')
    if read_sidecar(manifest_checksum, native['manifest'],
                    'manifest checksum') != manifest_digest:
        fail('publication release manifest checksum differs from the accepted manifest')
    validate_manifest(parse_json(manifest_bytes, 'publication release manifest'),
                      document, native)
    vsix_digest = digest_bytes(vsix_path.read_bytes())
    if vsix_digest != vsix['vsix_sha256']:
        fail('publication VSIX bytes differ from the handoff')
    validate_vsix_manifest(parse_json(vsix_manifest_path.read_bytes(),
                                      'publication VSIX manifest'), document, vsix)
    release_assets = (archive, checksum, manifest_path, manifest_checksum,
                      vsix_path)
    return {'vsix': vsix_path, 'release': release_assets}


def expected_release_assets(assets):
    """Return the exact GitHub-release asset name to digest mapping."""
    return {path.name: 'sha256:' + digest_bytes(path.read_bytes())
            for path in assets['release']}


def ensure_release(backend, repository, document, assets):
    """Create or accept the release only when it holds the exact accepted assets.

    An existing release is never overwritten: any extra, missing or differently
    hashed asset is a conflict and stops publication before the registry lanes.
    A newly created release is read back and must contain exactly those bytes.
    """
    tag = f'v{document["version"]}'
    expected = expected_release_assets(assets)
    existing = backend.release_assets(repository, tag)
    if existing is not None:
        if existing != expected:
            fail(f'GitHub release {tag} has conflicting assets; refusing to overwrite')
        return 'existing'
    backend.create_release(repository, tag, tag, list(assets['release']))
    verified = backend.release_assets(repository, tag)
    if verified != expected:
        fail(f'GitHub release {tag} does not contain the exact accepted assets')
    return 'created'


def registry_commands(vsix_path, root='.'):
    """Return the exact locked marketplace/Open VSX argv for one VSIX path."""
    base = Path(root)
    return (
        (str(base / VSCE), 'publish', '--oidc', '--packagePath', str(vsix_path)),
        (str(base / OVSX), 'publish', '--trusted-publishing', str(vsix_path)),
    )


class CommandBackend:
    """The real external boundary: ``gh`` and the locked local registry CLIs.

    Every command runs from the repository root with the workflow's inherited
    environment. ``vsce``/``ovsx`` come only from the frozen lockfile install;
    there is no ``npx``, global tool or PAT fallback.
    """

    def __init__(self, root='.'):
        self.root = Path(root)

    def _run(self, argv, label):
        result = subprocess.run(argv, cwd=str(self.root), capture_output=True,
                                text=True)
        if result.returncode != 0:
            detail = (result.stderr or result.stdout).strip().splitlines()
            fail(f'{label} exited {result.returncode}: ' +
                 ' | '.join(detail[-4:] or ['no output']))
        return result.stdout

    def release_assets(self, repository, tag):
        """Return the release's asset digests, or ``None`` when it is absent."""
        result = subprocess.run(
            ['gh', 'api', f'repos/{repository}/releases/tags/{tag}'],
            cwd=str(self.root), capture_output=True, text=True)
        if result.returncode != 0:
            message = result.stderr or result.stdout
            if 'HTTP 404' in message or 'Not Found' in message:
                return None
            fail(f'GitHub release lookup for {tag} failed: '
                 + ' | '.join(message.strip().splitlines()[-4:] or ['no output']))
        document = parse_json(result.stdout, f'GitHub release {tag}')
        entries = document.get('assets') if isinstance(document, dict) else None
        if not isinstance(entries, list):
            fail(f'GitHub release {tag} has no asset listing')
        assets = {}
        for entry in entries:
            if not isinstance(entry, dict) or not isinstance(entry.get('name'), str):
                fail(f'GitHub release {tag} has an invalid asset entry')
            assets[entry['name']] = entry.get('digest')
        return assets

    def create_release(self, repository, tag, title, assets):
        self._run(['gh', 'release', 'create', tag, '--verify-tag',
                   '--title', title, '--notes-from-tag',
                   *[str(path) for path in assets]], f'GitHub release {tag}')

    def publish_marketplace(self, vsix_path):
        self._run(registry_commands(vsix_path, self.root)[0],
                  'VS Code Marketplace publish')

    def publish_openvsx(self, vsix_path):
        self._run(registry_commands(vsix_path, self.root)[1],
                  'Open VSX publish')


def load_event(environment):
    """Return the trusted ``workflow_run`` event document from GITHUB_EVENT_PATH."""
    path = environment.get('GITHUB_EVENT_PATH', '')
    if not path:
        fail('GITHUB_EVENT_PATH is required for the workflow_run context')
    try:
        return parse_json(Path(path).read_bytes(), 'publication event')
    except OSError as error:
        fail(f'publication event is unreadable: {error}')


def publish(handoff_path, backend=None, environment=None, event=None):
    """Validate one handoff, then mutate GitHub, Marketplace and Open VSX.

    Returns the completed destination names in order. Any validation problem or
    destination failure raises :class:`PublishError` describing the completed
    and pending destinations; no destination is retried.
    """
    environment = dict(os.environ) if environment is None else environment
    document, root = load_handoff(handoff_path)
    validate_handoff_binding(document)
    event = load_event(environment) if event is None else event
    repository = trusted_context(document, environment, event)
    assets = load_assets(document, root)
    backend = backend or CommandBackend()
    try:
        ensure_release(backend, repository, document, assets)
    except PublishError as error:
        fail(f'publication failed before any registry: {error}; completed none; '
             f'pending {", ".join(DESTINATIONS)}')
    completed = ['github-release']
    lanes = (('marketplace', backend.publish_marketplace),
             ('openvsx', backend.publish_openvsx))
    for index, (name, lane) in enumerate(lanes):
        try:
            lane(assets['vsix'])
        except PublishError as error:
            pending = [pending for pending, _ in lanes[index:]]
            fail(f'publication partial: completed {", ".join(completed)}; '
                 f'pending {", ".join(pending)}; {name} failed: {error}')
        completed.append(name)
    return completed


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--handoff', required=True,
                        help='validated publication-handoff.json downloaded by '
                             'the protected publisher job')
    args = parser.parse_args(argv)
    try:
        completed = publish(args.handoff)
    except PublishError as error:
        print(f'FAILED: {error}', file=sys.stderr)
        return 1
    print(f'ok: published {", ".join(completed)} from {args.handoff}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
