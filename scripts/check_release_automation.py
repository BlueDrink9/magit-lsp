#!/usr/bin/env python3
"""Check the release build/publication workflows and Cargo publish metadata.

The default mode is a static, line-oriented check (PyYAML is not available) of
``.github/workflows/release.yml`` and ``.github/workflows/publish.yml`` plus
`cargo package --list`, so an accidental auto-publish, a publication step in the
build workflow, incomplete crate metadata or release provenance that is not
bound to one source commit cannot land unnoticed. The helper modes are invoked
by the workflows themselves: ``--validate-run-metadata`` verifies that the
selected build run is this repository's successful release build of the
dispatch commit, ``--check-release-target`` resolves the version tag's commit
and prints ``RELEASE_TARGET=<sha>``, ``--validate-release-ref`` validates the
version pair and tag ref for a build run, ``--validate-artifacts`` verifies the
downloaded release archive/sidecar set and version triple, and
``--print-version`` emits the ``RELEASE_VERSION`` environment line for later
jobs. The default mode also verifies that the build workflow and the local
`make release-locked`/`release-artifact-check` targets share one pinned
release pipeline script, and runs fixture-driven self-tests of those decision
points, so the rejections are exercised rather than only described.
``--automation-unit`` runs only the static workflow/pipeline guards and their
fixtures, with no Cargo metadata or package, so it is usable in a tools-only
shell. ``--validate-publication-binding BUNDLE`` is the strict data boundary:
it validates one producer run, every paginated job/artifact page, the upload
identity, the run-bound manifest and the peeled original tag together; it makes
no readiness claim and is not wired into publication yet.
"""

import argparse
import hashlib
import inspect
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

WORKFLOW = Path('.github/workflows/publish.yml')
BUILD_WORKFLOW = Path('.github/workflows/release.yml')
CARGO_MANIFEST = Path('Cargo.toml')
EXTENSION_MANIFEST = Path('clients/vscode/package.json')
ARTIFACT_NAME = 'linux-x86_64-gnu-release-candidate'
ASSET = 'x86_64-unknown-linux-gnu'
MAKEFILE = Path('Makefile')
PIPELINE = Path('scripts/check_release_pipeline.py')
# The workflow's fixed toolchain is the shared pin: the local pipeline script
# refuses any other version, and both invocations below are the only build and
# artifact paths the workflow may use.
RELEASE_TOOLCHAIN = '1.94.1'
PIPELINE_BUILD = 'python3 scripts/check_release_pipeline.py build'
# The removed local-compiler alias is retained only as a forbidden fixture, so
# the workflow and Makefile guards can prove they still refuse it.
PIPELINE_BUILD_LOCAL = 'python3 scripts/check_release_pipeline.py build --local'
PIPELINE_BUILD_NATIVE = 'python3 scripts/check_release_pipeline.py build-native'
PIPELINE_CHECK = 'python3 scripts/check_release_automation.py --pipeline-self-tests'
PIPELINE_AUTOMATION_UNIT = 'python3 scripts/check_release_automation.py --automation-unit'
PIPELINE_ARTIFACTS = 'python3 scripts/check_release_pipeline.py artifacts'
# The no-build release contract entry point selects these methods explicitly;
# the optional real-binary smoke needs an actual artifact and must stay out.
OFFLINE_RELEASE_TESTS = (
    'test_latest_and_pinned',
    'test_default_directory',
    'test_download_failures',
    'test_missing_asset',
    'test_bad_metadata_and_versions',
    'test_destination_failure_preserves_binary',
    'test_checksum_failures',
    'test_platform_failures',
    'test_archive_structure',
    'test_package_install_contract',
    'test_package_rejects_version_mismatch',
)
REAL_BINARY_TEST = 'test_real_binary_package_install_launch'
RELEASE_CONTRACT_UNIT = 'release-contract-unit'
RELEASE_AUTOMATION_UNIT = 'release-automation-unit'
BUILD_WORKFLOW_PATH = '.github/workflows/release.yml'
BUILD_WORKFLOW_NAME = 'Release artifact build (no publication)'
API_ROOT = 'https://api.github.com'
# The publish workflow's provenance gate: the selected run is verified before
# any of its bytes are downloaded, and the emitted target feeds the tag.
RUN_PROVENANCE = 'python3 scripts/check_release_automation.py --validate-run-metadata'
TAG_TARGET_CHECK = 'python3 scripts/check_release_automation.py --check-release-target'
VERSION_PATTERN = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)')

# Paths owned by the local operator, never by the workflows they build here.
UNTRACKED_PATHS = (
    'scripts/package-release.sh',
    'scripts/install.sh',
    'scripts/ci-local.sh',
    '.envrc',
)
RELEASE_FORBIDDEN_PATHS = ('.envrc', 'scripts/ci-local.sh')
# Only the crates.io lane takes a stored secret; the marketplace CLIs
# authenticate with OIDC trusted publishing, so a PAT secret is forbidden.
STORED_SECRETS = ('CARGO_REGISTRY_TOKEN',)
FORBIDDEN_SECRETS = ('VSCE_PAT', 'OVSX_PAT')
OIDC_PUBLISH_ARGS = {'vsce publish': '--oidc',
                     'ovsx publish': '--trusted-publishing'}
PUBLISH_COMMANDS = ('vsce publish', 'ovsx publish', 'cargo publish --locked')
# The publish job resolves vsce/ovsx from the extension's locked dev
# dependencies, so the frozen install must precede every publishing step.
DEPENDENCY_INSTALL = 'make vscode-deps'
PUBLISHING_STEPS = ('gh release create',) + PUBLISH_COMMANDS
# The release checker imports tomllib, so the build workflow must select a
# supported interpreter instead of trusting the Ubuntu 22.04 default python3.
SETUP_PYTHON = 'actions/setup-python@'
MIN_PYTHON = (3, 11)
PYTHON_CONSUMER = re.compile(r'\bpython3(?:\s|$)')
# The build workflow's producer roles. `source-checks` is the mandatory
# precondition; the fallback keeps the established rustup path the publisher may
# consume, while the native matrix stays an isolated non-gating experiment until
# NREL-17 supplies hosted proof.
SOURCE_CHECK_JOB = 'source-checks'
FALLBACK_JOB = 'linux-x86_64-gnu'
NATIVE_JOB = 'native-candidates'
FALLBACK_TARGET = ASSET
SOURCE_CHECK_TARGETS = ('format-check', 'hygiene', 'test-rust', 'test-lua')
NATIVE_MATRIX = (
    ('ubuntu-22.04', 'x86_64-unknown-linux-gnu'),
    ('macos-26', 'aarch64-apple-darwin'),
    ('macos-26-intel', 'x86_64-apple-darwin'),
)
# Every producer packages the exact source/ref/event/run identity it was
# triggered from; an inferred head_branch or workflow_run SHA is never accepted.
IDENTITY_ENV = (
    ('RELEASE_REF', '${{ github.ref }}'),
    ('RELEASE_SOURCE_SHA', '${{ github.sha }}'),
    ('RELEASE_EVENT', '${{ github.event_name }}'),
    ('RELEASE_RUN_ID', '${{ github.run_id }}'),
    ('RELEASE_RUN_ATTEMPT', '${{ github.run_attempt }}'),
)
UPLOAD_IDENTITY_FILE = 'build/release/upload-identity.json'
NODE_TOOLING = ('pnpm', 'npm ', 'vsce', 'ovsx')
# Publication binding: the tracked producer's mandatory jobs. The native matrix
# rows are explicit experiments and no VSIX or release-ready consumer exists
# yet, so those names stay outside the gate until NREL-19 promotes them; this
# checker makes no publication-readiness claim about them.
MANDATORY_RUN_JOBS = (SOURCE_CHECK_JOB, FALLBACK_JOB)
PENDING_READINESS_JOBS = ('native-candidates', 'vsix', 'release-ready')
REQUIRED_RUN_ARTIFACTS = (ARTIFACT_NAME, f'{ARTIFACT_NAME}-identity')
RUN_EVENT = 'push'
# The run-bound manifest fields emitted by scripts/check_release_pipeline.py.
# Publication validation compares the exact set, so a replaced or extended
# manifest cannot ride along.
MANIFEST_FIELDS = frozenset({
    'source_sha', 'original_ref', 'event', 'run_id', 'run_attempt',
    'version', 'lock_hashes', 'target', 'toolchain', 'archive_sha256',
    'executable_sha256',
})
ARTIFACT_DIGEST_PATTERN = re.compile(r'sha256:[0-9a-f]{64}')


class CheckError(Exception):
    """One failed check, reported without a traceback."""


def fail(message):
    raise CheckError(message)


def read_lines(path):
    try:
        return path.read_text(encoding='utf-8').splitlines()
    except OSError as error:
        fail(f'cannot read {path}: {error}')


def indent_of(line):
    return len(line) - len(line.lstrip(' '))


def code_lines(lines):
    """Return the lines that are neither blank nor comments."""
    return [line for line in lines if line.strip() and not line.lstrip().startswith('#')]


def block_after(lines, start, max_indent):
    """Return the lines nested below ``lines[start]`` up to ``max_indent``."""
    block = []
    for line in lines[start + 1:]:
        if line.strip() and indent_of(line) <= max_indent:
            break
        block.append(line)
    return block


def find_line(lines, text, context):
    for number, line in enumerate(lines, 1):
        if line.strip() == text:
            return number
    fail(f'{context}: missing {text!r}')


def cargo_versions():
    try:
        import tomllib
    except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
        fail('python3 >= 3.11 is required (tomllib)')
    text = CARGO_MANIFEST.read_text(encoding='utf-8')
    package = tomllib.loads(text)['package']
    return package


def check_workflow(path=WORKFLOW):
    lines = read_lines(path)
    text = '\n'.join(lines)
    code = code_lines(lines)

    if not lines or lines[0].strip() != 'name: Publish release artifacts':
        fail('workflow: expected the publish workflow name on the first line')

    # Triggers: workflow_dispatch only; nothing may run on push, tag or PR.
    try:
        on_index = lines.index('on:')
    except ValueError:
        fail('workflow: missing top-level `on:`')
    trigger_block = block_after(lines, on_index, 0)
    triggers = [line.strip()[:-1] for line in trigger_block
                if indent_of(line) == 2 and line.strip().endswith(':')]
    if triggers != ['workflow_dispatch']:
        fail(f'workflow: triggers must be exactly workflow_dispatch, found {triggers}')
    for line in lines:
        if line.lstrip().startswith('#'):
            continue
        if re.match(r'^\s*(push|pull_request|pull_request_target|release|schedule|workflow_run):',
                    line):
            fail(f'workflow: forbidden automatic trigger {line.strip()!r}')

    dispatch_index = next(i for i, line in enumerate(lines) if line == '  workflow_dispatch:')
    dispatch_block = block_after(lines, dispatch_index, 2)
    if '    inputs:' not in dispatch_block:
        fail('workflow: workflow_dispatch has no `inputs:`')
    for name, required_lines in (
        ('build_run_id', ('        required: true', '        type: string')),
        ('mode', ('        default: dry-run', '        type: choice',
                  '          - dry-run', '          - publish')),
    ):
        try:
            offset = dispatch_block.index(f'      {name}:')
        except ValueError:
            fail(f'workflow: missing workflow_dispatch input {name!r}')
        input_block = block_after(dispatch_block, offset, 6)
        for expected in required_lines:
            if expected not in input_block:
                fail(f'workflow: input {name!r} is missing {expected.strip()!r}')

    top_permissions = block_after(lines, find_line(lines, 'permissions:', 'workflow') - 1, 0)
    if '  contents: read' not in top_permissions:
        fail('workflow: top-level permissions must be `contents: read`')

    # Pinned actions: every `uses:` needs a full sha and a version comment.
    check_pinned_actions(lines, 'workflow')

    # No inline step may carry more than ten lines of script.
    check_inline_scripts(lines, 'workflow')

    # Untracked operator files must never be referenced.
    for forbidden in UNTRACKED_PATHS:
        if forbidden in text:
            fail(f'workflow: references operator-owned path {forbidden!r}')

    validate = require_job(lines, 'validate')
    publish = require_job(lines, 'publish')

    validate_permissions = block_after(
        validate, validate.index('    permissions:'), 4)
    if '      contents: read' not in validate_permissions:
        fail('workflow: validate job must have `contents: read`')
    publish_permissions = block_after(
        publish, publish.index('    permissions:'), 4)
    if '      contents: write' not in publish_permissions:
        fail('workflow: publish job must have `contents: write`')
    if text.count('contents: write') != 1:
        fail('workflow: `contents: write` must appear exactly once (publish job)')

    # `actions: read` is only for the cross-run artifact download in validate;
    # keep it least-privilege by refusing it anywhere else.
    if '      actions: read' not in validate_permissions:
        fail('workflow: validate job must grant `actions: read` for the cross-run download')
    if '      actions: read' in publish_permissions:
        fail('workflow: publish job must not grant `actions: read`')
    if sum(line.count('actions: read') for line in code) != 1:
        fail('workflow: `actions: read` must appear exactly once (validate job)')

    if '    needs: validate' not in publish:
        fail('workflow: publish job must declare `needs: validate`')
    if not any(re.match(r"^\s*if:\s*\$\{\{\s*inputs\.mode\s*==\s*'publish'\s*\}\}", line)
               for line in publish):
        fail("workflow: publish job must be gated on `inputs.mode == 'publish'`")

    # Validate lane consumes the build artifact and verifies it without rebuilding.
    validate_text = '\n'.join(validate)
    if f'name: {ARTIFACT_NAME}' not in validate_text:
        fail(f'workflow: validate job must download {ARTIFACT_NAME!r}')
    if 'run-id: ${{ inputs.build_run_id }}' not in validate_text:
        fail('workflow: validate job must download from `inputs.build_run_id`')
    if 'actions/download-artifact@' not in validate_text:
        fail('workflow: validate job must use actions/download-artifact')

    # Provenance: the selected run is verified before any of its bytes are used,
    # so a same-version run from another workflow, repository or commit cannot
    # supply the assets. The checkout is pinned to the dispatch source commit.
    if 'ref: ${{ github.sha }}' not in validate_text:
        fail('workflow: validate job must check out the dispatch source commit')
    if RUN_PROVENANCE not in validate_text:
        fail('workflow: validate job must verify the selected build run provenance')
    if 'BUILD_RUN_ID: ${{ inputs.build_run_id }}' not in validate_text:
        fail('workflow: provenance check must receive `inputs.build_run_id`')
    provenance_at = next(i for i, line in enumerate(validate) if RUN_PROVENANCE in line)
    for later in ('actions/download-artifact@', '--validate-artifacts',
                  'make vscode-package', 'name: validated-release-assets'):
        later_at = next(i for i, line in enumerate(validate) if later in line)
        if provenance_at > later_at:
            fail(f'workflow: provenance verification must precede {later!r} '
                 'in the validate job')

    if 'python3 scripts/check_release_automation.py --validate-artifacts' not in validate_text:
        fail('workflow: validate job must verify the archive and version triple')
    if 'make vscode-package' not in validate_text:
        fail('workflow: validate job must build the offline VSIX')
    if 'name: validated-release-assets' not in validate_text:
        fail('workflow: validate job must hand off validated assets to publish')
    # Exactly the validated names are handed off; a glob or an unlisted path
    # would let an unexpected file ride into the published release.
    if '--print-version >> "$GITHUB_ENV"' not in validate_text:
        fail('workflow: validate job must resolve RELEASE_VERSION for the handoff')
    for glob in ('dist/*', 'build/vscode/*'):
        if glob in validate_text:
            fail(f'workflow: validated handoff must not upload {glob!r}')
    for asset in (f'dist/magit-lsp-v${{{{ env.RELEASE_VERSION }}}}-{ASSET}.tar.gz',
                  f'dist/magit-lsp-v${{{{ env.RELEASE_VERSION }}}}-{ASSET}.tar.gz.sha256',
                  'build/vscode/magit-lsp-${{ env.RELEASE_VERSION }}.vsix'):
        if asset not in validate_text:
            fail(f'workflow: validated handoff must list {asset!r}')
    version_at = next(i for i, line in enumerate(validate)
                      if '--print-version >> "$GITHUB_ENV"' in line)
    handoff_at = next(i for i, line in enumerate(validate)
                      if 'name: validated-release-assets' in line)
    if version_at > handoff_at:
        fail('workflow: RELEASE_VERSION must be resolved before the asset handoff')

    # Publish lanes: mode gate plus credential presence, one readable skip per lane.
    publish_text = '\n'.join(publish)
    if 'ref: ${{ github.sha }}' not in publish_text:
        fail('workflow: publish job must check out the dispatch source commit')
    if '--validate-artifacts release-assets/dist' not in publish_text:
        fail('workflow: publish job must re-verify the handed-off artifact set')
    if 'release-assets/dist/*' in publish_text:
        fail('workflow: release creation must upload exactly the validated assets')
    if TAG_TARGET_CHECK not in publish_text:
        fail('workflow: publish job must resolve the version tag target')
    if '--target "$RELEASE_TARGET"' not in publish_text:
        fail('workflow: release creation must target the verified source commit')
    if f'stem="release-assets/dist/magit-lsp-v${{RELEASE_VERSION}}-{ASSET}.tar.gz"' \
            not in publish_text:
        fail('workflow: release creation must name the validated archive path')
    if '"$stem" "$stem.sha256"' not in publish_text:
        fail('workflow: release creation must upload the archive and its sidecar')
    revalidate_at = next(i for i, line in enumerate(publish)
                         if '--validate-artifacts release-assets/dist' in line)
    tag_target_at = next(i for i, line in enumerate(publish) if TAG_TARGET_CHECK in line)
    release_at = next(i for i, line in enumerate(publish)
                      if 'gh release create' in line and not line.lstrip().startswith('#'))
    if revalidate_at > release_at:
        fail('workflow: artifact re-verification must precede release creation')
    if tag_target_at > release_at:
        fail('workflow: tag target resolution must precede release creation')
    for secret in STORED_SECRETS:
        if f'{secret}: ${{{{ secrets.{secret} }}}}' not in publish_text:
            fail(f'workflow: publish job must read secrets.{secret} into env')
        if f"env.{secret} != ''" not in publish_text:
            fail(f'workflow: {secret} lane must be gated on env.{secret} != empty')
        if f"env.{secret} == ''" not in publish_text:
            fail(f'workflow: {secret} lane must report a readable skip when unset')
    for secret in FORBIDDEN_SECRETS:
        if f'secrets.{secret}' in publish_text:
            fail(f'workflow: publish job must not read secrets.{secret}; '
                 'marketplace publication uses OIDC trusted publishing')
    for command, oidc_arg in OIDC_PUBLISH_ARGS.items():
        command_at = next((i for i, line in enumerate(publish)
                           if command in line and not line.lstrip().startswith('#')),
                          None)
        if command_at is None:
            continue  # reported by the publish-command loop below
        command_indent = indent_of(publish[command_at])
        step_lines = [publish[command_at]]
        for line in publish[command_at + 1:]:
            if line.strip() and line.lstrip().startswith('- ') \
                    and indent_of(line) <= command_indent:
                break
            step_lines.append(line)
        if oidc_arg not in '\n'.join(step_lines):
            fail(f'workflow: {command!r} must authenticate with OIDC '
                 f'({oidc_arg}) instead of a stored token')
    for command in PUBLISH_COMMANDS:
        if command not in publish_text:
            fail(f'workflow: missing publish command {command!r}')
    if 'gh release create' not in publish_text:
        fail('workflow: publish job must create the GitHub Release')
    if DEPENDENCY_INSTALL not in publish_text:
        fail('workflow: publish job must install the frozen VSCode CLI dependencies '
             f'with {DEPENDENCY_INSTALL!r} before publishing')
    install_at = next(i for i, line in enumerate(publish)
                      if DEPENDENCY_INSTALL in line and not line.lstrip().startswith('#'))
    for step in PUBLISHING_STEPS:
        step_at = next(i for i, line in enumerate(publish)
                       if step in line and not line.lstrip().startswith('#'))
        if install_at > step_at:
            fail(f'workflow: {DEPENDENCY_INSTALL!r} must run before {step!r} '
                 'in the publish job')
    if 'release-assets/dist/' not in publish_text:
        fail('workflow: release creation must upload the validated dist assets')
    if 'python3 scripts/check_release_automation.py --print-version' not in publish_text:
        fail('workflow: publish job must resolve the release version from the manifests')


def require_job(lines, name):
    try:
        index = lines.index(f'  {name}:')
    except ValueError:
        fail(f'workflow: missing job {name!r}')
    block = block_after(lines, index, 2)
    return block


def check_pinned_actions(lines, context):
    """Every `uses:` needs a full action sha plus a `# vX.Y.Z` comment.

    A one- or two-component release tag (for example `v31`) is accepted because
    some upstream actions publish only those; the full SHA remains mandatory.
    """
    for number, line in enumerate(lines, 1):
        match = re.match(r'^\s*(?:-\s*)?uses:\s*(\S+)(.*)$', line)
        if not match:
            continue
        reference, rest = match.groups()
        if not re.fullmatch(r'[\w.\-]+/[\w.\-]+@[0-9a-f]{40}', reference):
            fail(f'{context}:{number}: action {reference!r} is not pinned to a full sha')
        if not re.search(r'#\s*v\d+(?:\.\d+){0,2}\s*$', rest):
            fail(f'{context}:{number}: pinned action needs a `# vX.Y.Z` comment')


def check_inline_scripts(lines, context):
    """No inline `run:` block step may carry more than ten lines of script."""
    for index, line in enumerate(lines):
        if not re.match(r'^\s*(?:-\s*)?run:\s*[|>]', line):
            continue
        owner = indent_of(line)
        body = block_after(lines, index, owner)
        script = [entry for entry in body if entry.strip() and not entry.strip().startswith('#')]
        if len(script) > 10:
            fail(f'{context}:{index + 1}: inline script has {len(script)} lines (max 10)')


def check_python_interpreter(lines, context='build workflow'):
    """Require a pinned setup-python >= 3.11 before python3 in each job.

    Steps that run inside the locked Nix dev shell get Python from the flake, so
    only bare ``python3`` consumers need their own pinned interpreter. Checking
    per job keeps a consumer in one job from borrowing another job's setup step.
    """
    found_consumer = False
    for name, block in workflow_jobs(lines).items():
        consumers = [index for index, line in enumerate(block)
                     if PYTHON_CONSUMER.search(line)
                     and not line.lstrip().startswith('#')
                     and 'nix develop' not in line]
        if not consumers:
            continue
        found_consumer = True
        setup_at = next((index for index, line in enumerate(block)
                         if f'uses: {SETUP_PYTHON}' in line), None)
        if setup_at is None:
            fail(f'{context}: job {name!r} uses python3 without a pinned '
                 'actions/setup-python step')
        if setup_at > consumers[0]:
            fail(f'{context}: job {name!r} must put actions/setup-python before its '
                 f'first python3 step (line {consumers[0] + 1})')
        version = None
        for entry in block_after(block, setup_at, indent_of(block[setup_at])):
            match = re.match(r"^\s*python-version:\s*['\"]?(\d+(?:\.\d+)*)", entry)
            if match:
                version = tuple(int(part) for part in match.group(1).split('.'))
                break
        if not version:
            fail(f'{context}: job {name!r} actions/setup-python must select an '
                 'explicit python-version')
        if version < MIN_PYTHON:
            selected = '.'.join(str(part) for part in version)
            required = '.'.join(str(part) for part in MIN_PYTHON)
            fail(f'{context}: job {name!r} selects Python {selected}, below the '
                 f'required {required}')
    if not found_consumer:
        fail(f'{context}: no python3 consumer found to order against')


def check_node_tooling(lines):
    """Any JavaScript tooling must select an explicit Node 24 interpreter."""
    text = '\n'.join(code_lines(lines))
    if not any(token in text for token in NODE_TOOLING):
        return
    setup_at = next((index for index, line in enumerate(lines)
                     if 'uses: actions/setup-node@' in line), None)
    if setup_at is None:
        fail('build workflow: JavaScript tooling needs a pinned '
             'actions/setup-node step')
    version = None
    for entry in block_after(lines, setup_at, indent_of(lines[setup_at])):
        match = re.match(r"^\s*node-version:\s*['\"]?(\d+(?:\.\d+)*)", entry)
        if match:
            version = tuple(int(part) for part in match.group(1).split('.'))
            break
    if version != (24,):
        fail('build workflow: JavaScript tooling must select Node 24 explicitly')


def step_artifact_name(lines, action, context):
    """Return the `name:` of the first step that uses ``action``."""
    for index, line in enumerate(lines):
        if f'uses: {action}' not in line:
            continue
        for candidate in lines[index + 1:]:
            if candidate.strip() and indent_of(candidate) < indent_of(line):
                break
            match = re.match(r'^\s*name:\s*(\S+)\s*$', candidate)
            if match:
                return match.group(1)
        fail(f'{context}: {action} step has no artifact `name:`')
    fail(f'{context}: missing {action}')


def workflow_jobs(lines):
    """Split a workflow into ``{job name: body lines}`` at two-space indentation."""
    try:
        jobs_index = lines.index('jobs:')
    except ValueError:
        fail('build workflow: missing top-level `jobs:`')
    jobs = {}
    current = None
    for line in lines[jobs_index + 1:]:
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        if indent_of(line) == 2 and line.rstrip().endswith(':'):
            current = line.strip()[:-1]
            jobs[current] = []
        elif current is not None:
            jobs[current].append(line)
    if not jobs:
        fail('build workflow: no jobs declared')
    return jobs


def job_step(block, needle, context):
    """Return the step lines (six-space list items) containing ``needle``."""
    for index, line in enumerate(block):
        if needle in line and not line.lstrip().startswith('#'):
            start = index
            while start > 0 and not re.match(r'^      - ', block[start]):
                start -= 1
            if not re.match(r'^      - ', block[start]):
                fail(f'{context}: {needle!r} is not inside a workflow step')
            end = index + 1
            while end < len(block) and not re.match(r'^      - ', block[end]):
                end += 1
            return block[start:end]
    fail(f'{context}: missing {needle!r}')


def step_environment(step, context):
    """Return the ``env:`` mapping of one workflow step."""
    for index, line in enumerate(step):
        if line.strip() != 'env:':
            continue
        owner = indent_of(line)
        environment = {}
        for entry in step[index + 1:]:
            if entry.strip() and indent_of(entry) <= owner:
                break
            match = re.match(r'^\s*([A-Za-z_][A-Za-z0-9_]*):\s*(.+?)\s*$', entry)
            if match:
                environment[match.group(1)] = match.group(2)
        return environment
    fail(f'{context}: step has no `env:` handoff')


def step_uploads(step, context):
    """Return the artifact ``name:`` and explicit ``path:`` entries of a step."""
    name = None
    paths = []
    for index, line in enumerate(step):
        if line.strip().startswith('name:'):
            name = line.split(':', 1)[1].strip()
        if line.strip() == 'path: |':
            owner = indent_of(line)
            for entry in step[index + 1:]:
                if entry.strip() and indent_of(entry) <= owner:
                    break
                if entry.strip():
                    paths.append(entry.strip())
        elif line.strip().startswith('path:'):
            value = line.split(':', 1)[1].strip()
            if value:
                paths.append(value)
    if name is None or not paths:
        fail(f'{context}: upload-artifact needs an explicit name and path list')
    return name, paths


def upload_steps(block):
    """Return every upload-artifact step in one job block."""
    steps = []
    for index, line in enumerate(block):
        if 'uses: actions/upload-artifact@' not in line \
                or line.lstrip().startswith('#'):
            continue
        start = index
        while start > 0 and not re.match(r'^      - ', block[start]):
            start -= 1
        end = index + 1
        while end < len(block) and not re.match(r'^      - ', block[end]):
            end += 1
        steps.append(block[start:end])
    return steps


def require_read_only(lines):
    """Every code permission grant must be read-only; no write scope may appear."""
    for line in code_lines(lines):
        if re.search(r':\s*write\b', line):
            fail(f'build workflow: permission {line.strip()!r} is not read-only')


def require_pinned_builders(jobs):
    """Reject mutable runner labels and require every job to select a builder."""
    for name, block in jobs.items():
        runner = next((line.split(':', 1)[1].strip() for line in block
                       if line.strip().startswith('runs-on:')), None)
        if runner is None:
            fail(f'build workflow: job {name!r} has no runs-on builder')
        if 'latest' in runner:
            fail(f'build workflow: job {name!r} uses the mutable runner {runner!r}; '
                 'pin an explicit runner version')


def check_uploads(block, target, context, experimental):
    """Require explicit candidate files plus immutable upload identity evidence."""
    steps = upload_steps(block)
    if len(steps) < 2:
        fail(f'build workflow: {context} must upload the candidate files and their '
             'immutable upload identity evidence')
    name, paths = step_uploads(steps[0], context)
    if name == '${{ matrix.artifact }}':
        pass  # each matrix artifact name is checked with the matrix definition
    elif experimental != ('experimental' in name):
        claim = 'mark' if experimental else 'not claim'
        fail(f'build workflow: {context} artifact {name!r} must {claim} an '
             'experimental candidate')
    joined = '\n'.join(paths)
    if '*' in joined:
        fail(f'build workflow: {context} must upload explicit files, never a glob')
    if 'env.RELEASE_VERSION' not in joined:
        fail(f'build workflow: {context} upload paths must bind the validated '
             'RELEASE_VERSION')
    if target == FALLBACK_TARGET:
        if f'-{target}.tar.gz' not in joined:
            fail(f'build workflow: {context} must upload the {target} archive')
    elif '${{ matrix.target }}' not in joined:
        fail(f'build workflow: {context} must upload the matrix target archive')
    for suffix in ('.tar.gz', '.tar.gz.sha256', 'release-manifest.json',
                   'release-manifest.json.sha256'):
        if not any(path.endswith(suffix) for path in paths):
            fail(f'build workflow: {context} must upload the explicit {suffix} file')
    identity_uploads = [path for step in steps
                        for path in step_uploads(step, context)[1]
                        if path == UPLOAD_IDENTITY_FILE]
    if not identity_uploads:
        fail(f'build workflow: {context} must upload {UPLOAD_IDENTITY_FILE!r}')

    identity_step = job_step(block, 'steps.upload.outputs.artifact-id', context)
    identity_env = step_environment(identity_step, context)
    expected_identity = {
        'ARTIFACT_ID': '${{ steps.upload.outputs.artifact-id }}',
        'ARTIFACT_DIGEST': '${{ steps.upload.outputs.artifact-digest }}',
        'RUN_ID': '${{ github.run_id }}',
        'RUN_ATTEMPT': '${{ github.run_attempt }}',
    }
    for key, expected in expected_identity.items():
        if identity_env.get(key) != expected:
            fail(f'build workflow: {context} upload evidence must bind {key} to '
                 f'{expected!r}')
    return name


def check_producer_handoff(block, target, context, experimental):
    """Require the run-bound identity handoff and the immutable upload evidence."""
    step = job_step(block, PIPELINE_ARTIFACTS, context)
    environment = step_environment(step, context)
    for key, expected in IDENTITY_ENV:
        if environment.get(key) != expected:
            fail(f'build workflow: {context} packaging must bind {key} to '
                 f'{expected!r} (run-bound provenance, not inferred)')
    if environment.get('RELEASE_BUILD_TARGET') != target:
        fail(f'build workflow: {context} packaging target is '
             f'{environment.get("RELEASE_BUILD_TARGET")!r}, expected {target!r}')
    return check_uploads(block, target, context, experimental)


def check_native_candidates(block):
    """Keep the native matrix an isolated, non-gating, locked experiment."""
    code = '\n'.join(code_lines(block))
    if not any(line.strip() == 'needs: source-checks' for line in block):
        fail('build workflow: the native producers must need source-checks')
    if not any(line.strip() == 'continue-on-error: true' for line in block):
        fail('build workflow: experimental native candidates must stay non-gating '
             'until NREL-17 promotes them')
    matrix = {}
    artifacts = {}
    for index, line in enumerate(block):
        match = re.match(r'^\s*- os:\s*(\S+)\s*$', line)
        if not match:
            continue
        target = None
        artifact = None
        for entry in block[index + 1:]:
            if entry.strip() and indent_of(entry) <= indent_of(line):
                break
            found = re.match(r'^\s*target:\s*(\S+)\s*$', entry)
            if found:
                target = found.group(1)
            found = re.match(r'^\s*artifact:\s*(\S+)\s*$', entry)
            if found:
                artifact = found.group(1)
        matrix[match.group(1)] = target
        artifacts[match.group(1)] = artifact
    expected = dict(NATIVE_MATRIX)
    if matrix != expected:
        fail('build workflow: native builder matrix must be exactly '
             + ', '.join(f'{os}/{target}' for os, target in NATIVE_MATRIX))
    for os, artifact in artifacts.items():
        if not artifact or 'experimental' not in artifact:
            fail(f'build workflow: native matrix builder {os!r} must upload an '
                 'explicitly experimental artifact')
    if 'nix develop --no-update-lock-file .#release-native' not in code:
        fail('build workflow: native candidates must build inside the locked '
             '.#release-native shell')
    if 'make release-native-build RELEASE_TARGET=${{ matrix.target }}' not in code:
        fail('build workflow: native candidates must run the shared native build for '
             'their matrix target')
    if PIPELINE_ARTIFACTS not in code:
        fail('build workflow: native candidates must package through the shared pipeline')
    check_producer_handoff(block, '${{ matrix.target }}', 'native candidates', True)


def check_build_workflow(path=BUILD_WORKFLOW):
    """Structure-check the build workflow: locked builders, source checks,
    run-bound explicit candidates and read-only, non-publishing scope."""
    lines = read_lines(path)
    text = '\n'.join(lines)

    find_line(lines, 'name: Release artifact build (no publication)', 'build workflow')

    try:
        on_index = lines.index('on:')
    except ValueError:
        fail('build workflow: missing top-level `on:`')
    trigger_block = block_after(lines, on_index, 0)
    triggers = [line.strip()[:-1] for line in trigger_block
                if indent_of(line) == 2 and line.strip().endswith(':')]
    if sorted(triggers) != ['push', 'workflow_dispatch']:
        fail('build workflow: triggers must be workflow_dispatch plus push tags, '
             f'found {triggers}')
    push_index = next(i for i, line in enumerate(lines) if line == '  push:')
    push_block = block_after(lines, push_index, 2)
    tags = next((line.strip() for line in push_block
                 if line.strip().startswith('tags:')), None)
    if tags is None or "'v*'" not in tags:
        fail("build workflow: push trigger must filter `tags: ['v*']`")
    for line in lines:
        if line.lstrip().startswith('#'):
            continue
        if re.match(r'^\s*(pull_request|pull_request_target|release|schedule|workflow_run):',
                    line):
            fail(f'build workflow: forbidden automatic trigger {line.strip()!r}')

    top_permissions = block_after(
        lines, find_line(lines, 'permissions:', 'build workflow') - 1, 0)
    if '  contents: read' not in top_permissions:
        fail('build workflow: top-level permissions must be `contents: read`')
    require_read_only(lines)

    check_pinned_actions(lines, 'build workflow')
    check_inline_scripts(lines, 'build workflow')

    jobs = workflow_jobs(lines)
    for required in (SOURCE_CHECK_JOB, FALLBACK_JOB, NATIVE_JOB):
        if required not in jobs:
            fail(f'build workflow: missing mandatory job {required!r}')
    require_pinned_builders(jobs)

    # Source checks reuse the CI Make targets in the locked dev shell; they never
    # invoke the release producer or become a second build path.
    source_text = '\n'.join(code_lines(jobs[SOURCE_CHECK_JOB]))
    if 'nix develop --no-update-lock-file -c make' not in source_text:
        fail('build workflow: source checks must run the shared Make targets '
             'inside the locked Nix dev shell')
    for target in SOURCE_CHECK_TARGETS:
        if target not in source_text:
            fail(f'build workflow: source checks must run `make {target}`')
    for forbidden in ('check_release_pipeline.py', 'release-locked',
                      'release-native-build', 'release-artifact-check'):
        if forbidden in source_text:
            fail('build workflow: source checks must not invoke the release '
                 f'producer ({forbidden!r})')

    # The strict Linux fallback is the established rustup build the publisher may
    # consume; it keeps the pinned toolchain and the shared pipeline ordering.
    fallback = jobs[FALLBACK_JOB]
    fallback_code = code_lines(fallback)
    fallback_text = '\n'.join(fallback_code)
    if not any(line.strip() == 'needs: source-checks' for line in fallback):
        fail('build workflow: the fallback producer must need source-checks')
    runner = next((line.split(':', 1)[1].strip() for line in fallback
                   if line.strip().startswith('runs-on:')), '')
    if runner != 'ubuntu-22.04':
        fail('build workflow: the strict Linux fallback must run on ubuntu-22.04')
    if f'rustup toolchain install {RELEASE_TOOLCHAIN} --profile minimal' \
            not in fallback_text:
        fail(f'build workflow: must install the pinned Rust {RELEASE_TOOLCHAIN} toolchain')
    if '--validate-release-ref' not in fallback_text:
        fail('build workflow: must validate the version/tag ref before building '
             '(it produces RELEASE_VERSION for $GITHUB_ENV)')
    if PIPELINE_BUILD not in fallback_text:
        fail(f'build workflow: must run the shared pinned build {PIPELINE_BUILD!r}')
    if PIPELINE_BUILD_LOCAL in fallback_text:
        fail('build workflow: must use the pinned toolchain, never the local '
             'installed-compiler mode')
    if PIPELINE_BUILD_NATIVE in fallback_text:
        fail('build workflow: the strict fallback must not run the experimental '
             'native-linker mode')
    if PIPELINE_ARTIFACTS not in fallback_text:
        fail(f'build workflow: must run the shared artifact check {PIPELINE_ARTIFACTS!r}')
    for alternate in ('cargo build', 'package-release.sh', 'MAGIT_LSP_TEST_BINARY'):
        if alternate in fallback_text:
            fail(f'build workflow: {alternate!r} belongs to the shared pipeline script, '
                 'not a separate workflow path')
    ref_at = next(i for i, line in enumerate(fallback_code) if '--validate-release-ref' in line)
    build_at = next(i for i, line in enumerate(fallback_code) if PIPELINE_BUILD in line)
    artifacts_at = next(i for i, line in enumerate(fallback_code)
                        if PIPELINE_ARTIFACTS in line)
    if not ref_at < build_at < artifacts_at:
        fail('build workflow: version validation must precede the pinned build, '
             'and the build must precede artifact packaging')
    fallback_name = check_producer_handoff(fallback, FALLBACK_TARGET, 'fallback', False)
    if fallback_name != ARTIFACT_NAME:
        fail(f'build workflow: uploads {fallback_name!r}, expected {ARTIFACT_NAME!r} '
             'so the publisher consumes the strict fallback')

    check_native_candidates(jobs[NATIVE_JOB])
    check_python_interpreter(lines)
    check_node_tooling(lines)

    # Build-only: no publication step, publishing credential, mutable-name
    # download or operator-owned path.
    for command in PUBLISH_COMMANDS + ('gh release create',):
        if command in text:
            fail(f'build workflow: publication step {command!r} is not allowed')
    if re.search(r'\bsecrets\.', text):
        fail('build workflow: must not read publishing secrets')
    if re.search(r'uses:\s*actions/download-artifact@', text):
        fail('build workflow: candidates must not be consumed by mutable name '
             'before the NREL-08 immutable-ID validation')
    if 'dist/*' in text:
        fail('build workflow: must never hand off a dist glob')
    for forbidden in RELEASE_FORBIDDEN_PATHS:
        if forbidden in text:
            fail(f'build workflow: references operator-owned path {forbidden!r}')


def load_pipeline(path=PIPELINE):
    """Import the release pipeline module (a mutation copy under self-test)."""
    import importlib.util
    spec = importlib.util.spec_from_file_location('release_pipeline_under_test', path)
    if spec is None or spec.loader is None:
        fail(f'release pipeline: cannot load {path}')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def pipeline_violations(module, environment):
    """Return the contract violations of a release pipeline module.

    ``environment`` supplies the PATH the strict pinned probe reads, so the
    self-tests can prove the fixture toolchain is still rejected. The module must
    keep one strict build, separate packaging from native execution and offer no
    permissive local compiler alias.
    """
    problems = []
    if getattr(module, 'TOOLCHAIN', None) != RELEASE_TOOLCHAIN:
        problems.append(f'toolchain pin {getattr(module, "TOOLCHAIN", None)!r} '
                        f'is not {RELEASE_TOOLCHAIN!r}')
    if getattr(module, 'TARGET', None) != ASSET:
        problems.append(f'target {getattr(module, "TARGET", None)!r} is not {ASSET!r}')
    expected_build = ['cargo', 'build', '--release', '--locked', '--target', ASSET]
    if module.build_command(['cargo']) != expected_build:
        problems.append(f'build command is {module.build_command(["cargo"])!r}, '
                        f'expected {expected_build!r}')
    source = inspect.getsource(module)
    if '--local' in source:
        problems.append('the pipeline must not offer a permissive --local build mode')
    for removed in ('installed_compiler', 'installed_prefix', 'compiler_for'):
        if removed in source:
            problems.append(f'the pipeline must not keep the local build path {removed}')
    try:
        pinned = module.compiler(environment)
    except module.PipelineError:
        pass
    else:
        if pinned != ['cargo', f'+{RELEASE_TOOLCHAIN}'] and pinned != ['cargo']:
            problems.append(f'pinned compiler prefix is {pinned!r}, expected the '
                            f'{RELEASE_TOOLCHAIN} pin')
    plan = {
        'identity': {'version': '1.2.3', 'target': ASSET},
        'binary': 'binary',
        'output': 'build/release/artifacts',
        'provenance_path': 'build/release/provenance.json',
    }
    steps, environment_vars = module.artifact_commands(plan)
    expected_package = ['sh', 'scripts/package-release.sh', '1.2.3', 'binary',
                        'build/release/artifacts', ASSET,
                        'build/release/provenance.json']
    if not steps or steps[0] != expected_package:
        problems.append('artifact steps must package the explicit binary with its '
                        'target and provenance record')
    if len(steps) != 2 or steps[1][-2:] != ['--validate-artifacts',
                                           'build/release/artifacts']:
        problems.append('artifact steps must validate the archive set offline')
    if environment_vars != {'PYTHONDONTWRITEBYTECODE': '1'}:
        problems.append('artifact steps must not hand the binary to a runtime test')
    for step in steps:
        joined = ' '.join(step)
        for forbidden in ('cargo', 'unittest', 'MAGIT_LSP_TEST_BINARY'):
            if forbidden in joined:
                problems.append(f'artifact step must not run {forbidden}')
    for name in ('validate_identity', 'resolve_identity', 'verify_source',
                 'verify_native_evidence', 'package_plan', 'release_manifest',
                 'manifest_digest', 'write_manifest'):
        if not callable(getattr(module, name, None)):
            problems.append(f'the pipeline is missing {name}')
    if getattr(module, 'NATIVE_CC', None) != '/usr/bin/cc':
        problems.append('the experimental native lane must use the host /usr/bin/cc')
    if getattr(module, 'NATIVE_LINKER_VARIABLE', None) != \
            'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER':
        problems.append('the experimental native lane must set the explicit target '
                        'linker variable')
    if getattr(module, 'NATIVE_XCRUN', None) != '/usr/bin/xcrun':
        problems.append('the Darwin native lane must resolve its toolchain through '
                        'the system /usr/bin/xcrun')
    targets = getattr(module, 'NATIVE_TARGETS', {})
    linux = targets.get(ASSET, {})
    if linux.get('host') != ('linux', 'x86_64') or linux.get('linker') != '/usr/bin/cc':
        problems.append('the native target table must keep the Linux x86_64 host '
                        'and explicit /usr/bin/cc')
    for target, host, label in (
        ('aarch64-apple-darwin', ('darwin', 'arm64'), 'Darwin arm64'),
        ('x86_64-apple-darwin', ('darwin', 'x86_64'), 'Darwin x86_64'),
    ):
        config = targets.get(target)
        if not config:
            problems.append(f'the native target table is missing the {label} '
                            f'target {target}')
            continue
        if config.get('host') != host:
            problems.append(f'the {label} target host is {config.get("host")!r}, '
                            f'expected {host!r}')
        if not config.get('deployment_target'):
            problems.append(f'the {label} target must record an explicit '
                            'deployment target')
        variable = config.get('linker_variable', '')
        if not variable.startswith('CARGO_TARGET_') or \
                not variable.endswith('_LINKER'):
            problems.append(f'the {label} target must set an explicit target '
                            'linker variable')
    return problems


def check_release_pipeline():
    """The local pipeline script must keep the workflow's pinned build/artifact contract."""
    problems = pipeline_violations(load_pipeline(), dict(os.environ))
    if problems:
        fail('release pipeline: ' + '; '.join(problems))


def make_recipe(lines, number):
    """Return the tab-indented recipe lines below the 1-based Makefile line ``number``.

    Make recipes are identified by a leading tab, not by indentation depth, so
    the YAML ``block_after`` helper cannot read them.
    """
    recipe = []
    for line in lines[number:]:
        if not line.startswith('\t'):
            break
        recipe.append(line)
    return recipe


def check_local_pipeline(path=MAKEFILE):
    """The Make targets must share the pipeline and offer no local build alias."""
    lines = read_lines(path)
    for removed in ('release-local', 'release-artifact-check-local'):
        if any(line.startswith(removed + ':') for line in lines):
            fail(f'Makefile: {removed} is a prohibited local build alias')
    for line in lines:
        if line.startswith('\t') and 'check_release_pipeline.py build --local' in line:
            fail('Makefile: the pipeline must not offer a permissive local build')
    for target, command in (
        ('release-locked', PIPELINE_BUILD),
        ('release-artifact-check', PIPELINE_ARTIFACTS),
        ('release-native-build', PIPELINE_BUILD_NATIVE),
        ('release-pipeline-check', PIPELINE_CHECK),
        ('release-automation-unit', PIPELINE_AUTOMATION_UNIT),
    ):
        number = next((index for index, line in enumerate(lines, 1)
                       if line.startswith(target + ':')), None)
        if number is None:
            fail(f'Makefile: missing {target}')
        recipe = make_recipe(lines, number)
        if not any(command in line for line in recipe):
            fail(f'Makefile: {target} must run {command!r}')
    find_line(lines, 'release-artifact-check: release-locked', 'Makefile')


def check_automation_unit(path=MAKEFILE):
    """`release-automation-unit` must run the static self-tests without Cargo."""
    lines = read_lines(path)
    number = next((index for index, line in enumerate(lines, 1)
                   if line.startswith(RELEASE_AUTOMATION_UNIT + ':')), None)
    if number is None:
        fail(f'Makefile: missing {RELEASE_AUTOMATION_UNIT}')
    recipe = '\n'.join(make_recipe(lines, number))
    if PIPELINE_AUTOMATION_UNIT not in recipe:
        fail(f'Makefile: {RELEASE_AUTOMATION_UNIT} must run '
             f'{PIPELINE_AUTOMATION_UNIT!r}')
    for forbidden in ('cargo', 'release-locked', 'release-native-build', '--release'):
        if forbidden in recipe:
            fail(f'Makefile: {RELEASE_AUTOMATION_UNIT} must not run {forbidden!r}')


def check_contract_unit(path=MAKEFILE):
    """`release-contract-unit` must select the offline cases explicitly, with no build."""
    lines = read_lines(path)
    number = next((index for index, line in enumerate(lines, 1)
                   if line.startswith(RELEASE_CONTRACT_UNIT + ':')), None)
    if number is None:
        fail(f'Makefile: missing {RELEASE_CONTRACT_UNIT}')
    recipe = '\n'.join(make_recipe(lines, number))
    for test in OFFLINE_RELEASE_TESTS:
        if f'tests.release.test_release.ReleaseTests.{test}' not in recipe:
            fail(f'Makefile: {RELEASE_CONTRACT_UNIT} must select '
                 f'tests.release.test_release.ReleaseTests.{test}')
    if REAL_BINARY_TEST in recipe:
        fail(f'Makefile: {RELEASE_CONTRACT_UNIT} must exclude the optional '
             f'real-binary {REAL_BINARY_TEST} case')
    if 'discover' in recipe:
        fail(f'Makefile: {RELEASE_CONTRACT_UNIT} must select cases explicitly, '
             'not discover them')
    if 'cargo' in recipe:
        fail(f'Makefile: {RELEASE_CONTRACT_UNIT} must not build or run Cargo')
    if 'shellcheck scripts/install.sh scripts/package-release.sh' not in recipe:
        fail(f'Makefile: {RELEASE_CONTRACT_UNIT} must lint the release shell scripts')


def check_artifact_consistency():
    """The build workflow must upload exactly the artifact the publish workflow downloads."""
    uploaded = step_artifact_name(
        read_lines(BUILD_WORKFLOW), 'actions/upload-artifact@', 'build workflow')
    downloaded = step_artifact_name(
        read_lines(WORKFLOW), 'actions/download-artifact@', 'publish workflow')
    if uploaded != ARTIFACT_NAME:
        fail(f'build workflow: uploads {uploaded!r}, expected {ARTIFACT_NAME!r}')
    if downloaded != ARTIFACT_NAME:
        fail(f'publish workflow: downloads {downloaded!r}, expected {ARTIFACT_NAME!r}')
    if uploaded != downloaded:
        fail(f'artifact name mismatch: release.yml uploads {uploaded!r} but '
             f'publish.yml downloads {downloaded!r}')


def check_metadata():
    package = cargo_versions()
    for field in ('description', 'license', 'repository', 'readme'):
        value = package.get(field)
        if not isinstance(value, str) or not value.strip():
            fail(f'Cargo.toml: package.{field} is missing or empty')
    readme = Path(package['readme'])
    if not readme.is_file():
        fail(f'Cargo.toml: package.readme {package["readme"]!r} does not exist')

    metadata = run_cargo(
        ['cargo', 'metadata', '--no-deps', '--offline', '--format-version', '1'],
        'cargo metadata')
    packages = json.loads(metadata)['packages']
    crate = next((entry for entry in packages
                  if entry['name'] == package.get('name')), None)
    if crate is None:
        fail(f'cargo metadata: package {package.get("name")!r} not found')
    for field in ('description', 'license', 'repository'):
        if not crate.get(field):
            fail(f'cargo metadata: resolved package field {field!r} is empty')
    readme_path = crate.get('readme')
    if not readme_path or not Path(readme_path).is_file():
        fail('cargo metadata: package readme does not resolve to a file')

    listing = run_cargo(
        ['cargo', 'package', '--list', '--allow-dirty', '--offline'],
        'cargo package --list')
    members = [line.strip() for line in listing.splitlines() if line.strip()]
    for required in ('Cargo.toml', 'src/lib.rs', 'src/main.rs', 'README.md'):
        if required not in members:
            fail(f'cargo package --list: missing publish file {required!r}')
    for member in members:
        for noisy in ('node_modules/', 'target/', 'build/'):
            if member.startswith(noisy) or f'/{noisy}' in member:
                fail(f'cargo package --list: unexpected local directory member {member!r}')


def run_cargo(command, label):
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip().splitlines()
        tail = detail[-6:] if detail else ['no output']
        fail(f'{label} exited {result.returncode}: ' + ' | '.join(tail))
    return result.stdout


def release_versions():
    package = cargo_versions()
    version = package.get('version', '')
    if not VERSION_PATTERN.fullmatch(version):
        fail(f'Cargo.toml: version {version!r} is not a stable X.Y.Z version')
    extension = json.loads(EXTENSION_MANIFEST.read_text(encoding='utf-8'))['version']
    if extension != version:
        fail(f'version mismatch: Cargo.toml {version} vs extension package.json {extension}')
    return version


def validate_release_ref():
    """Validate the version pair and tag ref for a build run (writes $GITHUB_ENV).

    Same semantics as the build workflow's original inline check: the Cargo
    version is stable ``X.Y.Z``, the extension version equals it, and a tag run
    must be ``refs/tags/v<version>``. Workflow-dispatch runs skip the tag rule.
    """
    package = cargo_versions()
    cargo = package.get('version', '')
    extension = json.loads(EXTENSION_MANIFEST.read_text(encoding='utf-8'))['version']
    if not VERSION_PATTERN.fullmatch(cargo):
        fail(f'expected stable X.Y.Z version, found {cargo!r}')
    if cargo != extension:
        fail(f'Cargo and extension versions differ ({cargo} vs {extension})')
    ref = os.environ.get('RELEASE_REF', '')
    if not ref:
        fail('RELEASE_REF is not set')
    if ref.startswith('refs/tags/') and ref != 'refs/tags/v' + cargo:
        fail(f'tag must match Cargo and extension versions: {ref} is not v{cargo}')
    target = os.environ.get('GITHUB_ENV')
    if not target:
        fail('GITHUB_ENV is not set')
    with open(target, 'a', encoding='utf-8') as out:
        out.write('RELEASE_VERSION=' + cargo + '\n')
    print(f'ok: RELEASE_VERSION={cargo} (ref {ref})')


def validate_artifacts(directory, version=None):
    """Verify the downloaded archive/sidecar set and version triple.

    ``version`` defaults to the manifests' version; the self-tests pass a fixed
    one so they do not depend on the checkout's current version. The directory
    must contain exactly the expected archive and sidecar: an unexpected file is
    refused rather than uploaded later, and the checksum is still compared.
    """
    version = version or release_versions()
    root = Path(directory)
    stem = f'magit-lsp-v{version}-{ASSET}'
    archive = root / f'{stem}.tar.gz'
    sidecar = root / f'{archive.name}.sha256'
    if not archive.is_file():
        fail(f'artifacts: missing {archive}')
    if not sidecar.is_file():
        fail(f'artifacts: missing {sidecar}')
    if root.is_dir():
        expected = {archive.name, sidecar.name}
        unexpected = sorted(entry.name for entry in root.iterdir()
                            if entry.name not in expected)
        if unexpected:
            fail(f'artifacts: unexpected files in {root}: ' + ', '.join(unexpected))

    sidecar_text = sidecar.read_text(encoding='utf-8')
    match = re.fullmatch(r'([0-9a-f]{64})  (\S+)\n', sidecar_text)
    if not match:
        fail(f'artifacts: {sidecar.name} must be `<sha256>  <archive>\\n`, '
             'with two spaces')
    digest, name = match.groups()
    if name != archive.name:
        fail(f'artifacts: sidecar names {name!r}, expected {archive.name!r}')
    actual = hashlib.sha256(archive.read_bytes()).hexdigest()
    if digest != actual:
        fail(f'artifacts: sha256 mismatch for {archive.name}')
    print(f'ok: validated {archive.name} ({version}, sha256 {digest})')


def parse_json(document, label):
    """Return the parsed JSON of ``document`` (bytes or text) or fail readably."""
    try:
        return json.loads(document)
    except (TypeError, ValueError) as error:
        fail(f'{label}: invalid JSON ({error})')


def full_name(value):
    """Return ``full_name`` from an API repository object, or ``None``."""
    return value.get('full_name') if isinstance(value, dict) else None


def fetch_api(repository, path, token):
    """Return ``(status, body)`` for one GitHub API GET, failing closed.

    Transport and credential problems are check failures, never an implicit
    "resource missing": only the API's own status decides absence.
    """
    if not token:
        fail('GH_TOKEN is required for GitHub API validation')
    request = urllib.request.Request(
        f'{API_ROOT}/repos/{repository}{path}',
        headers={'Accept': 'application/vnd.github+json',
                 'Authorization': f'Bearer {token}'})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()
    except OSError as error:
        fail(f'GitHub API request for {path} failed: {error}')


def validate_run_metadata(document, repository, sha):
    """Fail unless the run is this repository's successful release build of ``sha``.

    ``document`` is a decoded "Get a workflow run" response. Repository, workflow
    identity (path and name), completion, conclusion and head SHA must all match,
    so a same-version artifact from another workflow, repository, fork or commit
    can never be published as this commit's release.
    """
    if not isinstance(document, dict):
        fail('build run metadata: expected a JSON object')
    if document.get('path') != BUILD_WORKFLOW_PATH:
        fail(f'build run workflow is {document.get("path")!r}, '
             f'expected {BUILD_WORKFLOW_PATH!r}')
    if document.get('name') != BUILD_WORKFLOW_NAME:
        fail(f'build run workflow name is {document.get("name")!r}, '
             f'expected {BUILD_WORKFLOW_NAME!r}')
    run_repository = full_name(document.get('repository'))
    if run_repository != repository:
        fail(f'build run repository is {run_repository!r}, expected {repository!r}')
    head_repository = full_name(document.get('head_repository'))
    if head_repository != repository:
        fail(f'build run head repository is {head_repository!r}, '
             f'expected {repository!r}')
    if document.get('status') != 'completed':
        fail(f'build run status is {document.get("status")!r}, expected completed')
    if document.get('conclusion') != 'success':
        fail(f'build run conclusion is {document.get("conclusion")!r}, '
             'expected success')
    head_sha = document.get('head_sha')
    if head_sha != sha:
        fail(f'build run head sha is {head_sha!r}, '
             f'expected the dispatch checkout {sha!r}')


def release_target(status, body, sha):
    """Return the commit the version tag must point at, refusing a conflict.

    ``status``/``body`` are the ``commits/tags/v<version>`` lookup. HTTP 404 means
    the tag does not exist yet, so the new tag must be created at the verified
    source commit ``sha``; HTTP 200 is the tag's commit object, whose ``sha`` must
    already be that same commit. Any other status fails closed.
    """
    if status == 404:
        return sha
    if status != 200:
        fail(f'release tag lookup returned HTTP {status}')
    document = parse_json(body, 'release tag lookup')
    tag_sha = document.get('sha') if isinstance(document, dict) else None
    if not isinstance(tag_sha, str) or not re.fullmatch(r'[0-9a-f]{40}', tag_sha):
        fail('release tag lookup: missing commit sha')
    if tag_sha != sha:
        fail(f'release tag points at {tag_sha}, not the verified source {sha}')
    return sha


def run_int(value):
    """Return ``value`` as a non-negative integer run field, or fail readably."""
    if isinstance(value, str) and re.fullmatch(r'[0-9]+', value):
        return int(value)
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        fail(f'run binding: {value!r} is not a non-negative integer')
    return value


def validate_run_binding(document, repository, run_id, run_attempt, workflow_id,
                         tag_ref):
    """Fail unless the run is this repository's push-tag build of ``tag_ref``.

    Run id, attempt, repository, head repository, workflow path/name/id, event,
    completion, conclusion and the tag branch must all match. ``head_sha`` is
    deliberately not compared with the release source: the original tag's
    peeled commit is the only source oracle, so a default-branch SHA carried in
    the payload can never be mistaken for the release source. ``head_branch`` is
    only compared with the exact tag name declared by the trusted version.
    """
    if not isinstance(document, dict):
        fail('run binding: expected a JSON object')
    if document.get('path') != BUILD_WORKFLOW_PATH:
        fail(f'run binding workflow is {document.get("path")!r}, '
             f'expected {BUILD_WORKFLOW_PATH!r}')
    if document.get('name') != BUILD_WORKFLOW_NAME:
        fail(f'run binding workflow name is {document.get("name")!r}, '
             f'expected {BUILD_WORKFLOW_NAME!r}')
    if document.get('workflow_id') != workflow_id:
        fail(f'run binding workflow id is {document.get("workflow_id")!r}, '
             f'expected {workflow_id!r}')
    if full_name(document.get('repository')) != repository:
        fail(f'run binding repository is {full_name(document.get("repository"))!r}, '
             f'expected {repository!r}')
    if full_name(document.get('head_repository')) != repository:
        fail(f'run binding head repository is '
             f'{full_name(document.get("head_repository"))!r}, expected {repository!r}')
    if document.get('event') != RUN_EVENT:
        fail(f'run binding event is {document.get("event")!r}, expected {RUN_EVENT}')
    if not isinstance(tag_ref, str) or not tag_ref.startswith('refs/tags/'):
        fail(f'run binding tag ref is {tag_ref!r}, expected refs/tags/*')
    tag_name = tag_ref[len('refs/tags/'):]
    if document.get('head_branch') != tag_name:
        fail(f'run binding branch is {document.get("head_branch")!r}, '
             f'expected the tag {tag_name!r}')
    if document.get('id') != run_int(run_id):
        fail(f'run binding id is {document.get("id")!r}, expected {run_id!r}')
    if document.get('run_attempt') != run_int(run_attempt):
        fail(f'run binding attempt is {document.get("run_attempt")!r}, '
             f'expected {run_attempt!r}')
    if document.get('status') != 'completed':
        fail(f'run binding status is {document.get("status")!r}, expected completed')
    if document.get('conclusion') != 'success':
        fail(f'run binding conclusion is {document.get("conclusion")!r}, '
             'expected success')


def paginated_entries(pages, key, label):
    """Return the complete entry list of every ``pages`` document, or fail.

    GitHub paginates the job and artifact listings. A missing page, a page that
    disagrees about ``total_count`` or a duplicate entry id is a contradiction,
    not a partial success, so it fails closed. The collected count must equal
    ``total_count`` exactly.
    """
    if not isinstance(pages, list) or not pages:
        fail(f'{label}: no API pages were supplied')
    total = None
    entries = []
    seen = set()
    for index, page in enumerate(pages):
        if not isinstance(page, dict):
            fail(f'{label}: page {index + 1} is not a JSON object')
        count = page.get('total_count')
        if isinstance(count, bool) or not isinstance(count, int) or count < 0:
            fail(f'{label}: page {index + 1} has no valid total_count')
        if total is None:
            total = count
        elif count != total:
            fail(f'{label}: pages disagree on total_count ({total} vs {count})')
        page_entries = page.get(key)
        if not isinstance(page_entries, list):
            fail(f'{label}: page {index + 1} has no {key!r} list')
        for entry in page_entries:
            if not isinstance(entry, dict):
                fail(f'{label}: page {index + 1} has a non-object entry')
            identity = entry.get('id')
            if isinstance(identity, int) and not isinstance(identity, bool):
                if identity in seen:
                    fail(f'{label}: duplicate entry id {identity}')
                seen.add(identity)
            entries.append(entry)
    if len(entries) < total:
        fail(f'{label}: incomplete pagination (collected {len(entries)} of {total})')
    if len(entries) > total:
        fail(f'{label}: contradictory pagination (collected {len(entries)} '
             f'of {total})')
    return entries


def validate_run_jobs(pages, run_id, run_attempt, mandatory=MANDATORY_RUN_JOBS):
    """Require every page and every declared mandatory job to succeed.

    Every listed job must belong to ``run_id`` and ``run_attempt``. Each
    mandatory name must appear and every job with that name must be completed
    with conclusion ``success``; a missing, skipped, cancelled, neutral or
    otherwise inconclusive mandatory job fails the run.
    """
    jobs = paginated_entries(pages, 'jobs', 'build run jobs')
    expected_run = run_int(run_id)
    expected_attempt = run_int(run_attempt)
    by_name = {}
    for job in jobs:
        if job.get('run_id') != expected_run:
            fail(f'build run job {job.get("name")!r} belongs to run '
                 f'{job.get("run_id")!r}, expected {expected_run}')
        if job.get('run_attempt') != expected_attempt:
            fail(f'build run job {job.get("name")!r} belongs to attempt '
                 f'{job.get("run_attempt")!r}, expected {expected_attempt}')
        name = job.get('name')
        if not isinstance(name, str) or not name:
            fail('build run job has no name')
        by_name.setdefault(name, []).append(job)
    for name in mandatory:
        matches = by_name.get(name, [])
        if not matches:
            fail(f'build run is missing mandatory job {name!r}')
        for job in matches:
            if job.get('status') != 'completed':
                fail(f'mandatory job {name!r} is inconclusive '
                     f'(status {job.get("status")!r})')
            if job.get('conclusion') != 'success':
                fail(f'mandatory job {name!r} concluded '
                     f'{job.get("conclusion")!r}, not success')
    return jobs


def validate_manifest_binding(manifest, expected):
    """Fail unless the untrusted run manifest is the trusted exact binding.

    ``expected`` carries the trusted version, target, run identity, locks and
    toolchain. The manifest's keys must be exactly the producer contract's
    fields, and its tag ref must be the version's ``refs/tags/v*``. The source
    SHA is only shape-checked here and bound to the tag by
    :func:`validate_tag_binding`; manifest content is never executed.
    """
    if not isinstance(manifest, dict):
        fail('release manifest: expected a JSON object')
    keys = set(manifest)
    if keys != MANIFEST_FIELDS:
        missing = sorted(MANIFEST_FIELDS - keys)
        unexpected = sorted(keys - MANIFEST_FIELDS)
        fail(f'release manifest fields differ: missing {missing}, '
             f'unexpected {unexpected}')
    for field in ('run_id', 'run_attempt', 'event', 'version', 'target',
                  'original_ref'):
        if manifest.get(field) != expected[field]:
            fail(f'release manifest {field} is {manifest.get(field)!r}, '
                 f'expected {expected[field]!r}')
    if manifest['original_ref'] != f"refs/tags/v{manifest['version']}":
        fail(f'release manifest original_ref {manifest["original_ref"]!r} is not '
             f'the version tag v{manifest["version"]}')
    if not isinstance(manifest['source_sha'], str) or not re.fullmatch(
            r'[0-9a-f]{40}', manifest['source_sha']):
        fail('release manifest source_sha is not a full commit SHA')
    for field in ('archive_sha256', 'executable_sha256'):
        if not isinstance(manifest[field], str) or not re.fullmatch(
                r'[0-9a-f]{64}', manifest[field]):
            fail(f'release manifest {field} is not a SHA-256')
    if manifest['lock_hashes'] != expected['lock_hashes']:
        fail('release manifest lock_hashes differ from the checked-out source')
    if manifest['toolchain'] != expected['toolchain']:
        fail('release manifest toolchain differs from the trusted toolchain')


def validate_tag_binding(status, ref_body, tag_body, manifest):
    """Peel the manifest's original tag via the API and require its source.

    HTTP 404 means the original tag is missing, which publication cannot repair.
    A lightweight ref peels directly to its commit; an annotated ref must be
    peeled through its tag object. The peeled commit must equal the manifest
    source SHA, so a moved or replaced tag fails for both shapes.
    """
    original_ref = manifest['original_ref']
    tag_name = original_ref[len('refs/tags/'):]
    if status == 404:
        fail(f'release tag {original_ref} is missing; publication requires the '
             'original tag')
    if status != 200:
        fail(f'release tag {original_ref} lookup returned HTTP {status}')
    document = parse_json(ref_body, 'release tag ref')
    if not isinstance(document, dict):
        fail('release tag ref: expected a JSON object')
    if document.get('ref') != original_ref:
        fail(f'release tag ref is {document.get("ref")!r}, expected {original_ref!r}')
    target = document.get('object')
    if not isinstance(target, dict):
        fail(f'release tag {original_ref} has no object')
    kind = target.get('type')
    sha = target.get('sha')
    if not isinstance(sha, str) or not re.fullmatch(r'[0-9a-f]{40}', sha):
        fail(f'release tag {original_ref} has no full object sha')
    if kind == 'commit':
        peeled = sha
    elif kind == 'tag':
        if tag_body is None:
            fail(f'annotated release tag {original_ref} was not peeled')
        annotated = parse_json(tag_body, 'release tag object')
        if not isinstance(annotated, dict) or annotated.get('tag') != tag_name:
            fail(f'release tag object is not {tag_name!r}')
        commit = annotated.get('object')
        if not isinstance(commit, dict) or commit.get('type') != 'commit':
            fail(f'release tag {original_ref} does not peel to a commit')
        peeled = commit.get('sha')
        if not isinstance(peeled, str) or not re.fullmatch(r'[0-9a-f]{40}', peeled):
            fail(f'release tag {original_ref} peels to no full commit sha')
    else:
        fail(f'release tag {original_ref} points at an unsupported {kind!r} object')
    if peeled != manifest['source_sha']:
        fail(f'release tag {original_ref} peels to {peeled}, not the manifest '
             f'source {manifest["source_sha"]}')
    return peeled


def validate_run_artifacts(pages, run_id, required=REQUIRED_RUN_ARTIFACTS):
    """Return the run's artifacts keyed by name after a fail-closed check.

    Every required production artifact must appear exactly once with an integer
    id, a ``sha256:`` digest and a non-expired state, and any recorded
    ``workflow_run`` must be this run. Only the explicit ``native-*``
    experiments may coexist; any other unexpected name is refused.
    """
    artifacts = paginated_entries(pages, 'artifacts', 'build run artifacts')
    expected_run = run_int(run_id)
    by_name = {}
    for artifact in artifacts:
        name = artifact.get('name')
        if not isinstance(name, str) or not name:
            fail('build run artifact has no name')
        if isinstance(artifact.get('id'), bool) or not isinstance(
                artifact.get('id'), int):
            fail(f'build run artifact {name!r} has no integer id')
        if not ARTIFACT_DIGEST_PATTERN.fullmatch(str(artifact.get('digest'))):
            fail(f'build run artifact {name!r} has no supported digest')
        if artifact.get('expired') is not False:
            fail(f'build run artifact {name!r} is expired or has no expiry state')
        workflow_run = artifact.get('workflow_run')
        if workflow_run is not None:
            if not isinstance(workflow_run, dict) or \
                    run_int(workflow_run.get('id')) != expected_run:
                fail(f'build run artifact {name!r} belongs to another run')
        if name in by_name:
            fail(f'build run has duplicate artifact {name!r}')
        by_name[name] = artifact
    for name in required:
        if name not in by_name:
            fail(f'build run is missing required artifact {name!r}')
    for name in by_name:
        if name not in required and not name.startswith('native-'):
            fail(f'build run has an unexpected artifact {name!r}')
    return by_name


def validate_upload_identity(document, run_id, run_attempt, artifact):
    """Bind the downloaded upload identity to this run attempt and artifact.

    The identity file is untrusted data: it must carry exactly the producer's
    fields, name the same run/attempt and target, and reproduce the API
    artifact's id and digest.
    """
    if not isinstance(document, dict):
        fail('upload identity: expected a JSON object')
    expected_fields = {'artifact_id', 'artifact_digest', 'run_id', 'run_attempt',
                       'target'}
    if set(document) != expected_fields:
        fail(f'upload identity fields differ: {sorted(document)}')
    if document['run_id'] != str(run_int(run_id)):
        fail(f'upload identity run_id is {document["run_id"]!r}, expected {run_id!r}')
    if document['run_attempt'] != str(run_int(run_attempt)):
        fail(f'upload identity run_attempt is {document["run_attempt"]!r}, '
             f'expected {run_attempt!r}')
    if document['target'] != ASSET:
        fail(f'upload identity target is {document["target"]!r}, expected {ASSET!r}')
    if not ARTIFACT_DIGEST_PATTERN.fullmatch(str(document['artifact_digest'])):
        fail('upload identity artifact_digest is not a supported digest')
    if document['artifact_id'] != str(artifact.get('id')):
        fail(f'upload identity artifact id {document["artifact_id"]!r} does not '
             f'match API artifact {artifact.get("id")!r}')
    if document['artifact_digest'] != artifact.get('digest'):
        fail('upload identity artifact digest does not match the API artifact')


def validate_archive_binding(manifest, archive):
    """Require the downloaded archive bytes to hash to the manifest's record."""
    path = Path(archive)
    if not path.is_file():
        fail(f'release archive {path} is missing')
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != manifest['archive_sha256']:
        fail(f'release archive {path.name} does not match the release manifest hash')
    return digest


def validate_publication_binding(run_document, job_pages, artifact_pages,
                                 identity_document, manifest, trusted,
                                 repository, run_id, run_attempt, workflow_id,
                                 tag_status, tag_ref, tag_object=None):
    """Strict data-only entrypoint binding one producer run, tag and assets.

    The caller fetches the run, every jobs and artifacts page, the tag ref and
    (for an annotated tag) its tag object; all of it is untrusted data and none
    of it is executed. The trusted version, target, locks and toolchain arrive
    separately in ``trusted``. On success the validated source commit, jobs and
    artifacts are returned for privileged consumers; NREL-18 owns the workflow
    wiring that calls this after fetching, and no readiness is claimed here.
    """
    validate_run_binding(run_document, repository, run_id, run_attempt,
                         workflow_id, trusted.get('original_ref'))
    jobs = validate_run_jobs(job_pages, run_id, run_attempt)
    artifacts = validate_run_artifacts(artifact_pages, run_id)
    validate_manifest_binding(manifest, trusted)
    validate_upload_identity(identity_document, run_id, run_attempt,
                             artifacts[ARTIFACT_NAME])
    source = validate_tag_binding(tag_status, tag_ref, tag_object, manifest)
    return {'source_sha': source, 'jobs': jobs, 'artifacts': artifacts,
            'manifest': manifest}


def validate_publication_binding_bundle(path):
    """Validate an offline JSON publication-binding bundle and print its source.

    The bundle is the data form of :func:`validate_publication_binding`: the
    already-fetched run, job/artifact pages, upload identity, manifest, trusted
    fields and tag ref/object. It is the CLI boundary privileged consumers can
    call without embedding validation logic, and it never performs I/O beyond
    reading the bundle or executes any artifact content.
    """
    document = parse_json(Path(path).read_bytes(), 'publication binding bundle')
    if not isinstance(document, dict):
        fail('publication binding bundle: expected a JSON object')
    fields = {'repository', 'run_id', 'run_attempt', 'workflow_id', 'run',
              'job_pages', 'artifact_pages', 'identity', 'manifest', 'trusted',
              'tag_status', 'tag_ref', 'tag_object'}
    missing = sorted(fields - set(document))
    unexpected = sorted(set(document) - fields)
    if missing or unexpected:
        fail(f'publication binding bundle fields differ: missing {missing}, '
             f'unexpected {unexpected}')
    tag_ref = document['tag_ref']
    tag_object = document['tag_object']
    result = validate_publication_binding(
        document['run'], document['job_pages'], document['artifact_pages'],
        document['identity'], document['manifest'], document['trusted'],
        document['repository'], document['run_id'], document['run_attempt'],
        document['workflow_id'], document['tag_status'],
        json.dumps(tag_ref),
        None if tag_object is None else json.dumps(tag_object))
    print(f"ok: publication binding for run {document['run_id']} "
          f"attempt {document['run_attempt']} from source {result['source_sha']}")
    return result


def repository_and_sha():
    """Return ``(repository, sha)`` from the workflow environment, validated."""
    repository = os.environ.get('GITHUB_REPOSITORY', '')
    sha = os.environ.get('GITHUB_SHA', '')
    if not re.fullmatch(r'[\w.\-]+/[\w.\-]+', repository):
        fail(f'GITHUB_REPOSITORY is not a repository slug: {repository!r}')
    if not re.fullmatch(r'[0-9a-f]{40}', sha):
        fail(f'GITHUB_SHA is not a full 40-character commit sha: {sha!r}')
    return repository, sha


def validate_build_run():
    """Verify the selected build run's provenance as the validate job's gate."""
    repository, sha = repository_and_sha()
    run_id = os.environ.get('BUILD_RUN_ID', '')
    if not re.fullmatch(r'[0-9]+', run_id):
        fail(f'BUILD_RUN_ID is not a run id: {run_id!r}')
    status, body = fetch_api(repository, f'/actions/runs/{run_id}',
                             os.environ.get('GH_TOKEN', ''))
    if status != 200:
        fail(f'build run {run_id}: API returned HTTP {status}')
    validate_run_metadata(parse_json(body, 'build run metadata'), repository, sha)
    print(f'ok: build run {run_id} is this repository\'s successful build of {sha}')


def check_release_target():
    """Return the verified release target, refusing a conflicting version tag."""
    repository, sha = repository_and_sha()
    version = release_versions()
    # `tags/` keeps a branch of the same name from being resolved instead.
    status, body = fetch_api(repository, f'/commits/tags/v{version}',
                             os.environ.get('GH_TOKEN', ''))
    target = release_target(status, body, sha)
    print(f'ok: release tag v{version} targets {target}', file=sys.stderr)
    return target


SELF_TEST_REPOSITORY = 'example/magit-lsp'
SELF_TEST_SHA = 'a' * 40
SELF_TEST_OTHER_SHA = 'b' * 40


def expect_check(label, function, *args, expected=None):
    """Fail the self-test unless ``function(*args)`` raises the intended check."""
    try:
        function(*args)
    except CheckError as error:
        if expected is not None and expected not in str(error):
            fail(f'self-test: {label} failed for another reason: {error}')
        return
    fail(f'self-test: {label} was accepted')


def expect_pipeline_error(module, label, function, *args, expected=None):
    """Fail the self-test unless the pipeline module rejects ``function(*args)``."""
    try:
        function(*args)
    except module.PipelineError as error:
        if expected is not None and expected not in str(error):
            fail(f'self-test: {label} failed for another reason: {error}')
        return
    fail(f'self-test: {label} was accepted')


def run_metadata_fixture(**overrides):
    """Return a successful build-run document for the self-tests."""
    document = {
        'name': BUILD_WORKFLOW_NAME,
        'path': BUILD_WORKFLOW_PATH,
        'status': 'completed',
        'conclusion': 'success',
        'head_sha': SELF_TEST_SHA,
        'repository': {'full_name': SELF_TEST_REPOSITORY},
        'head_repository': {'full_name': SELF_TEST_REPOSITORY},
    }
    document.update(overrides)
    return document


def tag_document(sha):
    """Return the tag lookup body for an existing tag pointing at ``sha``."""
    return json.dumps({'sha': sha})


SELF_TEST_WORKFLOW_ID = 987654321
SELF_TEST_RUN_ID = '555000111'
SELF_TEST_RUN_ATTEMPT = '1'
SELF_TEST_VERSION = '1.2.3'
SELF_TEST_TAG_REF = f'refs/tags/v{SELF_TEST_VERSION}'
SELF_TEST_LOCK_HASHES = {
    'Cargo.lock': '1' * 64,
    'flake.lock': '2' * 64,
    'clients/vscode/pnpm-lock.yaml': '3' * 64,
}
SELF_TEST_TOOLCHAIN = {'rust': RELEASE_TOOLCHAIN}
SELF_TEST_ARTIFACT_DIGEST = 'sha256:' + 'd' * 64


def run_binding_fixture(**overrides):
    """Return a coherent push-tag producer run for the publication fixtures."""
    document = {
        'id': int(SELF_TEST_RUN_ID),
        'run_attempt': int(SELF_TEST_RUN_ATTEMPT),
        'event': 'push',
        'head_branch': f'v{SELF_TEST_VERSION}',
        'head_sha': SELF_TEST_SHA,
        'path': BUILD_WORKFLOW_PATH,
        'name': BUILD_WORKFLOW_NAME,
        'workflow_id': SELF_TEST_WORKFLOW_ID,
        'status': 'completed',
        'conclusion': 'success',
        'repository': {'full_name': SELF_TEST_REPOSITORY},
        'head_repository': {'full_name': SELF_TEST_REPOSITORY},
    }
    document.update(overrides)
    return document


def job_document(name, job_id, **overrides):
    """Return one completed successful job document for this fixture run."""
    document = {
        'id': job_id,
        'run_id': int(SELF_TEST_RUN_ID),
        'run_attempt': int(SELF_TEST_RUN_ATTEMPT),
        'name': name,
        'status': 'completed',
        'conclusion': 'success',
    }
    document.update(overrides)
    return document


def page_document(key, entries, total=None):
    """Return one GitHub listing page with an explicit total count."""
    return {'total_count': len(entries) if total is None else total, key: list(entries)}


def artifact_document(name, artifact_id, **overrides):
    """Return one unexpired run artifact bound to the fixture run."""
    document = {
        'id': artifact_id,
        'name': name,
        'digest': SELF_TEST_ARTIFACT_DIGEST,
        'expired': False,
        'workflow_run': {'id': int(SELF_TEST_RUN_ID)},
    }
    document.update(overrides)
    return document


def manifest_fixture(**overrides):
    """Return the exact run-bound manifest contract for the fixtures."""
    document = {
        'source_sha': SELF_TEST_SHA,
        'original_ref': SELF_TEST_TAG_REF,
        'event': 'push',
        'run_id': SELF_TEST_RUN_ID,
        'run_attempt': SELF_TEST_RUN_ATTEMPT,
        'version': SELF_TEST_VERSION,
        'lock_hashes': dict(SELF_TEST_LOCK_HASHES),
        'target': ASSET,
        'toolchain': dict(SELF_TEST_TOOLCHAIN),
        'archive_sha256': 'c' * 64,
        'executable_sha256': 'e' * 64,
    }
    document.update(overrides)
    return document


def manifest_expectation(**overrides):
    """Return the trusted fields the manifest must reproduce exactly."""
    document = {
        'run_id': SELF_TEST_RUN_ID,
        'run_attempt': SELF_TEST_RUN_ATTEMPT,
        'event': 'push',
        'version': SELF_TEST_VERSION,
        'target': ASSET,
        'original_ref': SELF_TEST_TAG_REF,
        'lock_hashes': dict(SELF_TEST_LOCK_HASHES),
        'toolchain': dict(SELF_TEST_TOOLCHAIN),
    }
    document.update(overrides)
    return document


def tag_ref_body(kind='commit', sha=SELF_TEST_SHA, ref=SELF_TEST_TAG_REF):
    """Return a ``git/ref/tags`` body for a lightweight or annotated tag."""
    return json.dumps({'ref': ref, 'object': {'type': kind, 'sha': sha}})


def tag_object_body(commit=SELF_TEST_SHA, tag=f'v{SELF_TEST_VERSION}'):
    """Return the annotated tag object body that peels to ``commit``."""
    return json.dumps({'tag': tag, 'object': {'type': 'commit', 'sha': commit}})


def write_artifacts(directory, version='1.2.3', payload=b'archive bytes'):
    """Create the expected archive/sidecar pair and return the archive name."""
    root = Path(directory)
    root.mkdir(parents=True, exist_ok=True)
    archive = root / f'magit-lsp-v{version}-{ASSET}.tar.gz'
    archive.write_bytes(payload)
    digest = hashlib.sha256(payload).hexdigest()
    (root / f'{archive.name}.sha256').write_text(
        f'{digest}  {archive.name}\n', encoding='utf-8')
    return archive.name


def write_fake_toolchain(directory):
    """Write installed-looking cargo/rustc stubs and return their environment.

    The stubs report versions unlike the release pin and refuse the ``+1.94.1``
    probe, so a fixture can prove the strict pinned mode still rejects a foreign
    compiler.
    """
    root = Path(directory) / 'toolchain-bin'
    root.mkdir(parents=True, exist_ok=True)
    for tool, version in (('rustc', 'rustc 1.99.1 (fixture)'),
                          ('cargo', 'cargo 1.99.0 (fixture)')):
        script = root / tool
        script.write_text(
            '#!/bin/sh\n'
            'if [ "$1" = "+1.94.1" ]; then\n'
            '  echo "error: toolchain +1.94.1 is not installed" >&2\n'
            '  exit 1\n'
            'fi\n'
            f'echo "{version}"\n',
            encoding='utf-8')
        script.chmod(0o755)
    return {'PATH': str(root)}


def mutated_file(source, directory, label, mutations):
    """Write a mutated copy of ``source`` for the self-tests and return its path.

    Each mutation is ``(old, new, count)``; an ``old`` fragment that is absent
    fails the self-test, so a probe can never quietly stop exercising the guard.
    """
    text = source.read_text(encoding='utf-8')
    for old, new, count in mutations:
        if old not in text:
            fail(f'self-test: {label} cannot apply mutation {old!r}')
        text = text.replace(old, new) if count is None else text.replace(old, new, count)
    prefix = re.sub(r'[^a-z0-9]+', '-', label.lower())
    path = Path(directory) / f'{prefix}{source.suffix}'
    path.write_text(text, encoding='utf-8')
    return path


def mutated_workflow(directory, label, mutations):
    """Write a mutated copy of the publish workflow and return its path."""
    return mutated_file(WORKFLOW, directory, 'publish-' + label, mutations)


def fake_native_probe(calls, missing=(), xcrun_missing=(), xcrun_paths=None):
    """Return a probe that records argv and answers host-tool queries with fixtures.

    The pipeline self-tests pass this instead of a real process boundary, so they
    can prove the native lane's argv, refusals and evidence without invoking a
    compiler, Cargo or a build. ``xcrun_missing`` names failing xcrun queries and
    ``xcrun_paths`` overrides the resolved clang/ld/SDK paths.
    """
    resolved = {
        'clang': '/usr/bin/clang',
        'ld': '/usr/bin/ld',
        'sdk': '/Applications/Xcode.app/Contents/Developer/Platforms/'
               'MacOSX.platform/Developer/SDKs/MacOSX.sdk',
    }
    resolved.update(xcrun_paths or {})

    def probe(argv, environment=None):
        calls.append(list(argv))
        if missing and argv[0] in missing:
            return 127, '', f'fixture {argv[0]} is missing'
        basename = os.path.basename(argv[0])
        if basename == 'xcrun':
            query = tuple(argv[1:])
            if query in xcrun_missing:
                return 127, '', 'fixture xcrun query is missing'
            if query == ('--sdk', 'macosx', '--find', 'clang'):
                return 0, resolved['clang'] + '\n', ''
            if query == ('--sdk', 'macosx', '--find', 'ld'):
                return 0, resolved['ld'] + '\n', ''
            if query == ('--sdk', 'macosx', '--show-sdk-path'):
                return 0, resolved['sdk'] + '\n', ''
            if query == ('--sdk', 'macosx', '--show-sdk-version'):
                return 0, '26.0\n', ''
            return 127, '', 'missing fixture xcrun query'
        if basename == 'clang' and argv[1:] == ['--version']:
            return 0, ('Apple clang version 17.0.0 (clang-1700.0.13.3)\n'
                       'Target: arm64-apple-darwin26.0.0\n'), ''
        if basename == 'ld' and argv[1:] == ['-v']:
            return 0, '', '@(#)PROGRAM:ld PROJECT:ld-1115.7.3\n'
        if argv[1:] == ['--version'] and basename in ('cc', 'gcc'):
            return 0, 'cc (Ubuntu 11.4.0-1ubuntu1~22.04) 11.4.0\n', ''
        if argv[:1] == ['ldd'] and argv[1:] == ['--version']:
            return 0, 'ldd (Ubuntu GLIBC 2.35-0ubuntu3.1) 2.35\n', ''
        if argv[:2] == ['git', 'rev-parse']:
            return 0, SELF_TEST_SHA + '\n', ''
        if argv[:1] == ['rustc']:
            return 0, 'rustc 1.94.1 (fixture)\n', ''
        if argv[:1] == ['cargo']:
            return 0, 'cargo 1.94.1 (fixture)\n', ''
        return 127, '', 'missing fixture tool'
    return probe


def run_pipeline_self_tests():
    """Exercise the pipeline adapter with fake processes; never build or run Cargo."""
    with tempfile.TemporaryDirectory(prefix='magit-lsp-release-pipeline-') as temporary:
        directory = Path(temporary)
        toolchain = write_fake_toolchain(directory)
        pipeline = load_pipeline()
        problems = pipeline_violations(pipeline, dict(os.environ))
        if problems:
            fail('release pipeline: ' + '; '.join(problems))

        # Wiring: the shared targets run the same script; the native target is
        # explicit, the contract unit selects only offline cases and the
        # automation unit stays Cargo-free.
        check_local_pipeline()
        check_contract_unit()
        check_automation_unit()
        for label, mutations in (
            ('Makefile without the native build target',
             ((PIPELINE_BUILD_NATIVE, 'true', None),)),
            ('Makefile without the pipeline self-test target',
             ((PIPELINE_CHECK, 'true', None),)),
            ('Makefile without the pinned build target',
             ((PIPELINE_BUILD, 'cargo build --release', None),)),
            ('Makefile reintroducing the local build target',
             (('release-locked:\n',
               'release-local:\n'
               '\tpython3 scripts/check_release_pipeline.py build --local\n'
               'release-locked:\n', 1),)),
        ):
            expect_check(label, check_local_pipeline,
                         mutated_file(MAKEFILE, directory, label, mutations),
                         expected='Makefile:')
        expect_check(
            'Makefile with a Cargo automation unit', check_automation_unit,
            mutated_file(MAKEFILE, directory, 'Makefile with a Cargo automation unit',
                         ((PIPELINE_AUTOMATION_UNIT,
                           f'{PIPELINE_AUTOMATION_UNIT} && cargo metadata', None),)),
            expected=RELEASE_AUTOMATION_UNIT)
        expect_check(
            'Makefile with broad contract discovery', check_contract_unit,
            mutated_file(MAKEFILE, directory, 'Makefile with broad contract discovery',
                         (('tests.release.test_release.ReleaseTests.test_platform_failures',
                           'discover -s tests/release', 1),)),
            expected=RELEASE_CONTRACT_UNIT)
        expect_check(
            'Makefile with the real-binary contract case', check_contract_unit,
            mutated_file(MAKEFILE, directory, 'Makefile with the real-binary contract case',
                         ((f'tests.release.test_release.ReleaseTests.'
                           f'test_package_rejects_version_mismatch',
                           f'tests.release.test_release.ReleaseTests.'
                           f'test_package_rejects_version_mismatch '
                           f'tests.release.test_release.ReleaseTests.{REAL_BINARY_TEST}', 1),)),
            expected=RELEASE_CONTRACT_UNIT)

        # The shared script keeps the locked flags and the single strict build,
        # and the experimental lane keeps its explicit native linker identity.
        for label, mutations, expected in (
            ('pipeline without --locked',
             (("'--locked', ", '', None),), 'build command is'),
            ('pipeline reintroducing a local build alias',
             (("    parser.add_argument('--target', default='',",
               "    parser.add_argument('--local', action='store_true')\n"
               "    parser.add_argument('--target', default='',", 1),),
             'permissive --local'),
            ('pipeline without the explicit native linker variable',
             (("NATIVE_LINKER_VARIABLE = "
               "'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'",
               "NATIVE_LINKER_VARIABLE = 'OTHER_LINKER'", None),),
             'explicit target linker'),
            ('pipeline without the Darwin arm64 target',
             (("'aarch64-apple-darwin': {", "'aarch64-apple-other': {", 1),),
             'Darwin arm64 target'),
            ('pipeline without an explicit Darwin deployment target',
             (("'deployment_target': DARWIN_DEPLOYMENT_TARGETS['aarch64-apple-darwin'],",
               "'deployment_target': '',", 1),),
             'deployment target'),
        ):
            problems = pipeline_violations(
                load_pipeline(mutated_file(PIPELINE, directory, label, mutations)),
                toolchain)
            if not any(expected in problem for problem in problems):
                fail(f'self-test: {label} was accepted')

        # The strict 1.94.1 fallback is unchanged and is the only build: its
        # command keeps the locked target build, refuses a foreign rustc and the
        # module exposes no installed-compiler path.
        expected_pinned = ['cargo', '+1.94.1', 'build', '--release', '--locked',
                           '--target', ASSET]
        if pipeline.build_command(['cargo', '+1.94.1']) != expected_pinned:
            fail('self-test: the strict pinned fallback build command changed')
        expect_pipeline_error(pipeline, 'pinned fallback accepting a foreign toolchain',
                              pipeline.compiler, toolchain,
                              expected='pinned release build needs')
        for removed in ('compiler_for', 'installed_compiler', 'installed_prefix'):
            if hasattr(pipeline, removed):
                fail(f'self-test: the pipeline still exposes {removed}')

        # Separate package planning: a valid exact identity is accepted and a
        # missing or contradictory target/ref/version/source is refused before
        # the packager runs. The manifest binds the run identity and the archive
        # and executable digests; its digest stays outside the payload.
        version = pipeline.manifest_version()
        identity = {
            'target': ASSET,
            'original_ref': f'refs/tags/v{version}',
            'version': version,
            'source_sha': SELF_TEST_SHA,
            'event': 'push',
            'run_id': '123456',
            'run_attempt': '1',
        }
        fake_binary = directory / 'magit-lsp'
        fake_binary.write_bytes(b'fixture release binary bytes')
        output = directory / 'artifacts'
        plan = pipeline.package_plan(identity, binary=str(fake_binary),
                                     output=str(output))
        expected_sha = hashlib.sha256(fake_binary.read_bytes()).hexdigest()
        if plan['identity'] != identity:
            fail('self-test: package planning must normalize the exact identity')
        if plan['provenance'] != {'version': version, 'target': ASSET,
                                  'executable_sha256': expected_sha}:
            fail('self-test: package planning must bind the explicit provenance')
        steps, step_environment = pipeline.artifact_commands(plan)
        if steps[0] != ['sh', 'scripts/package-release.sh', version,
                        str(fake_binary), str(output.resolve()), ASSET,
                        plan['provenance_path']]:
            fail(f'self-test: package step is {steps[0]!r}')
        if len(steps) != 2 or steps[1][-2:] != ['--validate-artifacts',
                                               str(output.resolve())]:
            fail('self-test: packaging must validate the archive set offline')
        if step_environment != {'PYTHONDONTWRITEBYTECODE': '1'}:
            fail('self-test: packaging must not hand the binary to a test process')
        for step in steps:
            if any(part in ' '.join(step) for part in
                   ('cargo', 'unittest', 'MAGIT_LSP_TEST_BINARY')):
                fail('self-test: packaging steps must not compile or execute')
        for key in pipeline.REQUIRED_IDENTITY:
            incomplete = dict(identity)
            incomplete.pop(key)
            expect_pipeline_error(
                pipeline, f'package identity without {key}',
                pipeline.validate_identity, incomplete, expected=key)
        for label, broken, expected in (
            ('package identity with an unsupported target',
             dict(identity, target='aarch64-unknown-linux-gnu'), 'unsupported'),
            ('package identity with a contradictory tag ref',
             dict(identity, original_ref='refs/tags/v9.9.9'), 'does not name'),
            ('package identity with a malformed version',
             dict(identity, version='1.2'), 'stable X.Y.Z'),
            ('package identity with a contradictory manifest version',
             dict(identity, version='9.9.9'), 'manifest version'),
            ('package identity with a malformed source',
             dict(identity, source_sha='not-a-sha'), 'full commit SHA'),
        ):
            expect_pipeline_error(pipeline, label, pipeline.validate_identity,
                                  broken, expected=expected)
        expect_pipeline_error(
            pipeline, 'package identity from a foreign source',
            lambda: pipeline.verify_source(
                dict(identity, source_sha=SELF_TEST_OTHER_SHA), {},
                probe=fake_native_probe([])),
            expected='does not match the checked-out source')
        pipeline.verify_source(identity, {}, probe=fake_native_probe([]))
        expect_pipeline_error(
            pipeline, 'package planning into the protected dist directory',
            lambda: pipeline.package_plan(identity, binary=str(fake_binary),
                                          output=str(pipeline.ROOT / 'dist')),
            expected='protected output')
        archive_sha = 'c' * 64
        manifest = pipeline.release_manifest(plan, archive_sha)
        for field in ('source_sha', 'original_ref', 'event', 'run_id',
                      'run_attempt', 'version', 'lock_hashes', 'target',
                      'toolchain', 'archive_sha256', 'executable_sha256'):
            if not manifest.get(field):
                fail(f'self-test: release manifest is missing {field!r}')
        if manifest['source_sha'] != SELF_TEST_SHA or \
                manifest['original_ref'] != identity['original_ref']:
            fail('self-test: release manifest must bind the source and original ref')
        if set(manifest['lock_hashes']) != {'Cargo.lock', 'flake.lock',
                                           'clients/vscode/pnpm-lock.yaml'}:
            fail('self-test: release manifest must hash Cargo, flake and pnpm locks')
        if manifest['executable_sha256'] != expected_sha or \
                manifest['archive_sha256'] != archive_sha:
            fail('self-test: release manifest must bind archive and executable hashes')
        if 'manifest_sha256' in manifest or 'digest' in manifest:
            fail('self-test: the manifest must not contain its own digest')
        manifest_path, digest = pipeline.write_manifest(
            plan, archive_sha, directory=directory / 'manifest')
        if json.loads(manifest_path.read_text(encoding='utf-8')) != manifest:
            fail('self-test: the release manifest must round-trip')
        sidecar = (manifest_path.parent / (manifest_path.name + '.sha256')).read_text(
            encoding='utf-8')
        if sidecar != f'{digest}  {manifest_path.name}\n':
            fail('self-test: the external manifest digest must match the payload')
        if hashlib.sha256(manifest_path.read_bytes()).hexdigest() != digest:
            fail('self-test: the manifest digest must hash the written payload')
        provenance = pipeline.write_provenance(plan, directory=directory / 'manifest')
        if json.loads(provenance.read_text(encoding='utf-8')) != plan['provenance']:
            fail('self-test: the packager provenance must round-trip')

        # Native plan: explicit host linker, locked argv and recorded evidence.
        # The host boundary is a fixture executable so this self-test is
        # independent of whether the running machine has /usr/bin/cc.
        calls = []
        host = ('linux', 'x86_64')
        fake_cc = directory / 'cc'
        fake_cc.write_text('#!/bin/sh\n', encoding='utf-8')
        fake_cc.chmod(0o755)
        plan = pipeline.native_plan(pipeline.TARGET, {}, host=host,
                                    compiler_path=str(fake_cc),
                                    probe=fake_native_probe(calls))
        expected_argv = ['cargo', 'build', '--release', '--locked', '--target', ASSET]
        if plan['argv'] != expected_argv:
            fail(f'self-test: native build argv is {plan["argv"]!r}')
        if plan['environment'].get(pipeline.NATIVE_LINKER_VARIABLE) != '/usr/bin/cc':
            fail('self-test: native mode must set the explicit target linker to /usr/bin/cc')
        evidence = plan['evidence']
        for field in ('target', 'compiler', 'linker', 'libc', 'argv', 'source_sha',
                      'lock_hashes'):
            if not evidence.get(field):
                fail(f'self-test: native evidence is missing {field!r}')
        if evidence['target'] != ASSET or evidence['argv'] != expected_argv:
            fail('self-test: native evidence must record the target and exact argv')
        if evidence['linker'] != '/usr/bin/cc':
            fail('self-test: native evidence must record the explicit linker')
        if evidence['source_sha'] != SELF_TEST_SHA:
            fail('self-test: native evidence must record the source SHA')
        if set(evidence['lock_hashes']) != {'Cargo.lock', 'flake.lock'}:
            fail('self-test: native evidence must hash Cargo.lock and flake.lock')
        if any(call[:2] == ['cargo', 'build'] for call in calls):
            fail('self-test: the native plan must not run the build')
        path = pipeline.write_evidence(evidence, directory=directory / 'evidence')
        if json.loads(path.read_text(encoding='utf-8')) != evidence:
            fail('self-test: the native evidence file must round-trip the record')

        # Darwin native plans: both matching hosts resolve Apple Clang, ld and the
        # Xcode SDK through the injected system xcrun and record the explicit
        # deployment target next to the locked target argv.
        fake_xcrun = directory / 'xcrun'
        fake_xcrun.write_text('#!/bin/sh\n', encoding='utf-8')
        fake_xcrun.chmod(0o755)
        expected_sdk = {
            'path': '/Applications/Xcode.app/Contents/Developer/Platforms/'
                    'MacOSX.platform/Developer/SDKs/MacOSX.sdk',
            'version': '26.0',
        }
        for target, darwin_host, variable, deployment in (
            ('aarch64-apple-darwin', ('darwin', 'arm64'),
             'CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER', '11.0'),
            ('x86_64-apple-darwin', ('darwin', 'x86_64'),
             'CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER', '10.12'),
        ):
            calls = []
            plan = pipeline.native_plan(
                target, {}, host=darwin_host, xcrun_path=str(fake_xcrun),
                probe=fake_native_probe(calls))
            expected_argv = ['cargo', 'build', '--release', '--locked',
                             '--target', target]
            if plan['argv'] != expected_argv:
                fail(f'self-test: {target} native build argv is {plan["argv"]!r}')
            if plan['environment'].get(variable) != '/usr/bin/clang':
                fail(f'self-test: {target} must set the target linker to Apple Clang')
            if plan['environment'].get('MACOSX_DEPLOYMENT_TARGET') != deployment:
                fail(f'self-test: {target} must set the explicit deployment target')
            evidence = plan['evidence']
            for field in ('target', 'compiler', 'compiler_path', 'linker',
                          'linker_version', 'sdk', 'deployment_target',
                          'deployment_target_source', 'deployment_target_status',
                          'argv', 'source_sha', 'lock_hashes'):
                if not evidence.get(field):
                    fail(f'self-test: {target} native evidence is missing {field!r}')
            if evidence['target'] != target or evidence['argv'] != expected_argv:
                fail(f'self-test: {target} evidence must record the target and argv')
            if evidence['compiler_path'] != '/usr/bin/clang' or \
                    evidence['linker'] != '/usr/bin/ld':
                fail(f'self-test: {target} evidence must record the xcrun paths')
            if not evidence['compiler'].startswith('Apple clang version'):
                fail(f'self-test: {target} evidence must record the Apple Clang version')
            if evidence['sdk'] != expected_sdk:
                fail(f'self-test: {target} evidence must record the SDK path and version')
            if evidence['deployment_target'] != deployment:
                fail(f'self-test: {target} evidence must record its deployment target')
            if 'not proof' not in evidence['deployment_target_source'] or \
                    'not a tested' not in evidence['deployment_target_status']:
                fail(f'self-test: {target} must label the deployment target as a '
                     'proposed build baseline, not proven final macOS support')
            if any(call[:2] == ['cargo', 'build'] for call in calls):
                fail('self-test: the Darwin native plan must not run the build')
            path = pipeline.write_evidence(
                evidence, directory=directory / f'evidence-{target}')
            if json.loads(path.read_text(encoding='utf-8')) != evidence:
                fail(f'self-test: the {target} evidence file must round-trip')

        for label, target, darwin_host, expected in (
            ('Darwin arm64 target on an Intel host', 'aarch64-apple-darwin',
             ('darwin', 'x86_64'), 'Darwin arm64 host'),
            ('Darwin x86_64 target on an arm64 host', 'x86_64-apple-darwin',
             ('darwin', 'arm64'), 'Darwin x86_64 host'),
            ('Darwin arm64 target on a Linux host', 'aarch64-apple-darwin',
             ('linux', 'x86_64'), 'Darwin arm64 host'),
        ):
            expect_pipeline_error(
                pipeline, label,
                lambda target=target, darwin_host=darwin_host: pipeline.native_plan(
                    target, {}, host=darwin_host, xcrun_path=str(fake_xcrun),
                    probe=fake_native_probe([])),
                expected=expected)
        expect_pipeline_error(
            pipeline, 'Darwin native mode without the system xcrun',
            lambda: pipeline.native_plan(
                'aarch64-apple-darwin', {}, host=('darwin', 'arm64'),
                xcrun_path=str(directory / 'missing-xcrun'),
                probe=fake_native_probe([])),
            expected='missing or not executable')
        for label, query, expected in (
            ('Darwin native mode without the Xcode SDK',
             ('--sdk', 'macosx', '--show-sdk-path'), 'Xcode macOS SDK'),
            ('Darwin native mode without Apple Clang',
             ('--sdk', 'macosx', '--find', 'clang'), 'Apple Clang'),
            ('Darwin native mode without the Apple linker',
             ('--sdk', 'macosx', '--find', 'ld'), 'Apple linker'),
        ):
            expect_pipeline_error(
                pipeline, label,
                lambda query=query: pipeline.native_plan(
                    'aarch64-apple-darwin', {}, host=('darwin', 'arm64'),
                    xcrun_path=str(fake_xcrun),
                    probe=fake_native_probe([], xcrun_missing=(query,))),
                expected=expected)
        for label, paths in (
            ('Darwin native mode with Nix-resolved Apple Clang',
             {'clang': '/nix/store/ffff-apple-clang/bin/clang'}),
            ('Darwin native mode with a Nix-resolved SDK',
             {'sdk': '/nix/store/ffff-apple-sdk'}),
        ):
            expect_pipeline_error(
                pipeline, label,
                lambda paths=paths: pipeline.native_plan(
                    'aarch64-apple-darwin', {}, host=('darwin', 'arm64'),
                    xcrun_path=str(fake_xcrun),
                    probe=fake_native_probe([], xcrun_paths=paths)),
                expected='Nix store')
        for label, variable, value in (
            ('Darwin native mode with an inherited deployment target',
             'MACOSX_DEPLOYMENT_TARGET', '10.15'),
            ('Darwin native mode with an inherited SDK root', 'SDKROOT',
             '/nix/store/ffff-apple-sdk'),
            ('Darwin native mode with a Nix target linker',
             'CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER',
             '/nix/store/ffff-apple-clang/bin/clang'),
        ):
            expect_pipeline_error(
                pipeline, label,
                lambda env={variable: value}: pipeline.native_plan(
                    'aarch64-apple-darwin', env, host=('darwin', 'arm64'),
                    xcrun_path=str(fake_xcrun), probe=fake_native_probe([])),
                expected='inherited compiler/linker/sysroot')

        expect_pipeline_error(
            pipeline, 'native mode on a non-Linux host',
            lambda: pipeline.native_plan(ASSET, {}, host=('darwin', 'arm64'),
                                         probe=fake_native_probe([])),
            expected='Linux x86_64 host')
        expect_pipeline_error(
            pipeline, 'native mode for a cross target',
            lambda: pipeline.native_plan('aarch64-unknown-linux-gnu', {}, host=host,
                                         probe=fake_native_probe([])),
            expected='only supports')
        for label, variable, value in (
            ('native mode with an inherited C compiler wrapper', 'CC',
             '/nix/store/ffff-cc-wrapper/bin/cc'),
            ('native mode with inherited C flags sysroot', 'CFLAGS',
             '--sysroot=/nix/store/ffff-glibc'),
            ('native mode with inherited linker sysroot', 'LDFLAGS',
             '-Wl,--sysroot=/nix/store/ffff-glibc'),
            ('native mode with an inherited Nix C compiler', 'NIX_CC',
             '/nix/store/ffff-gcc-wrapper/bin/cc'),
            ('native mode with a Nix target linker',
             'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER',
             '/nix/store/ffff-gcc-wrapper/bin/ld'),
            ('native mode with an inherited SDK root', 'SDKROOT',
             '/nix/store/ffff-apple-sdk'),
        ):
            expect_pipeline_error(
                pipeline, label,
                lambda env={variable: value}: pipeline.native_plan(
                    ASSET, env, host=host, probe=fake_native_probe([])),
                expected='inherited compiler/linker/sysroot')
        expect_pipeline_error(
            pipeline, 'native mode without the host C compiler',
            lambda: pipeline.native_plan(ASSET, {}, host=host,
                                         compiler_path=str(directory / 'missing-cc'),
                                         probe=fake_native_probe([])),
            expected='missing or not executable')
        expect_pipeline_error(
            pipeline, 'native mode without the host libc probe',
            lambda: pipeline.native_plan(ASSET, {}, host=host,
                                         compiler_path=str(fake_cc),
                                         probe=fake_native_probe([], missing=('ldd',))),
            expected='ldd --version failed')
        expect_pipeline_error(
            pipeline, 'native mode without git provenance',
            lambda: pipeline.native_plan(ASSET, {}, host=host,
                                         compiler_path=str(fake_cc),
                                         probe=fake_native_probe([], missing=('git',))),
            expected='git rev-parse HEAD failed')

    print('ok: release pipeline adapter self-tests (fake processes; no build)')


def run_self_tests():
    """Exercise the provenance, tag, artifact and wiring guards with fixtures."""
    validate_run_metadata(run_metadata_fixture(), SELF_TEST_REPOSITORY, SELF_TEST_SHA)
    for label, document, expected in (
        ('wrong repository', run_metadata_fixture(repository={'full_name': 'other/repo'}),
         'build run repository is'),
        ('fork head repository',
         run_metadata_fixture(head_repository={'full_name': 'someone/magit-lsp'}),
         'head repository'),
        ('missing repository', run_metadata_fixture(repository=None),
         'build run repository is'),
        ('wrong workflow path',
         run_metadata_fixture(path='.github/workflows/other-build.yml'),
         'build run workflow is'),
        ('wrong workflow name', run_metadata_fixture(name='Some other build'),
         'workflow name is'),
        ('unfinished run', run_metadata_fixture(status='in_progress', conclusion=None),
         'build run status is'),
        ('unsuccessful run', run_metadata_fixture(conclusion='failure'),
         'build run conclusion is'),
        ('other source commit', run_metadata_fixture(head_sha=SELF_TEST_OTHER_SHA),
         'head sha is'),
    ):
        expect_check(label, validate_run_metadata, document,
                     SELF_TEST_REPOSITORY, SELF_TEST_SHA, expected=expected)
    expect_check('non-object run metadata', validate_run_metadata, [],
                 SELF_TEST_REPOSITORY, SELF_TEST_SHA, expected='expected a JSON object')

    # Publication binding: a coherent push-tag run, every paginated job and
    # artifact entry, the exact trusted manifest and both tag shapes are
    # accepted together. The native matrix stays a non-gating experiment and
    # the release-ready/VSIX contract is explicitly pending, so this fixture
    # makes no publication-readiness claim.
    if set(MANDATORY_RUN_JOBS) & set(PENDING_READINESS_JOBS):
        fail('self-test: pending readiness jobs must not be mandatory yet')
    validate_run_binding(run_binding_fixture(), SELF_TEST_REPOSITORY,
                         SELF_TEST_RUN_ID, SELF_TEST_RUN_ATTEMPT,
                         SELF_TEST_WORKFLOW_ID, SELF_TEST_TAG_REF)
    # A payload default-branch SHA is never the source oracle: changing it does
    # not change the binding, and the tag peel below still decides the source.
    validate_run_binding(run_binding_fixture(head_sha=SELF_TEST_OTHER_SHA),
                         SELF_TEST_REPOSITORY, SELF_TEST_RUN_ID,
                         SELF_TEST_RUN_ATTEMPT, SELF_TEST_WORKFLOW_ID,
                         SELF_TEST_TAG_REF)
    for label, document, expected in (
        ('foreign repository',
         run_binding_fixture(repository={'full_name': 'other/repo'}),
         'run binding repository is'),
        ('fork head repository',
         run_binding_fixture(head_repository={'full_name': 'someone/magit-lsp'}),
         'run binding head repository is'),
        ('foreign workflow path',
         run_binding_fixture(path='.github/workflows/other-build.yml'),
         'run binding workflow is'),
        ('foreign workflow name',
         run_binding_fixture(name='Some other build'),
         'run binding workflow name is'),
        ('foreign workflow id',
         run_binding_fixture(workflow_id=SELF_TEST_WORKFLOW_ID + 1),
         'run binding workflow id is'),
        ('manual dispatch origin',
         run_binding_fixture(event='workflow_dispatch'), 'expected push'),
        ('pull request origin',
         run_binding_fixture(event='pull_request'), 'expected push'),
        ('branch origin', run_binding_fixture(head_branch='main'),
         'expected the tag'),
        ('wrong run id', run_binding_fixture(id=int(SELF_TEST_RUN_ID) + 1),
         'run binding id is'),
        ('wrong run attempt',
         run_binding_fixture(run_attempt=int(SELF_TEST_RUN_ATTEMPT) + 1),
         'run binding attempt is'),
        ('unfinished run',
         run_binding_fixture(status='in_progress', conclusion=None),
         'run binding status is'),
        ('unsuccessful run', run_binding_fixture(conclusion='failure'),
         'run binding conclusion is'),
    ):
        expect_check(label, validate_run_binding, document, SELF_TEST_REPOSITORY,
                     SELF_TEST_RUN_ID, SELF_TEST_RUN_ATTEMPT,
                     SELF_TEST_WORKFLOW_ID, SELF_TEST_TAG_REF, expected=expected)
    expect_check('non-object run binding', validate_run_binding, [],
                 SELF_TEST_REPOSITORY, SELF_TEST_RUN_ID, SELF_TEST_RUN_ATTEMPT,
                 SELF_TEST_WORKFLOW_ID, SELF_TEST_TAG_REF,
                 expected='expected a JSON object')

    coherent_jobs = [job_document(SOURCE_CHECK_JOB, 1),
                     job_document(FALLBACK_JOB, 2)]
    jobs = validate_run_jobs(
        [page_document('jobs', coherent_jobs + [
            job_document('native-candidates', 3, conclusion='skipped')])],
        SELF_TEST_RUN_ID, SELF_TEST_RUN_ATTEMPT)
    if [job['name'] for job in jobs][:2] != list(MANDATORY_RUN_JOBS):
        fail('self-test: coherent jobs must list the mandatory producer jobs')
    for label, pages, expected in (
        ('missing mandatory job',
         [page_document('jobs', [job_document(SOURCE_CHECK_JOB, 1)])],
         'missing mandatory job'),
        ('skipped mandatory job',
         [page_document('jobs', [
             job_document(SOURCE_CHECK_JOB, 1),
             job_document(FALLBACK_JOB, 2, conclusion='skipped')])],
         'not success'),
        ('cancelled mandatory job',
         [page_document('jobs', [
             job_document(SOURCE_CHECK_JOB, 1),
             job_document(FALLBACK_JOB, 2, conclusion='cancelled')])],
         'not success'),
        ('neutral mandatory job',
         [page_document('jobs', [
             job_document(SOURCE_CHECK_JOB, 1),
             job_document(FALLBACK_JOB, 2, conclusion='neutral')])],
         'not success'),
        ('inconclusive mandatory job',
         [page_document('jobs', [
             job_document(SOURCE_CHECK_JOB, 1),
             job_document(FALLBACK_JOB, 2, status='in_progress',
                          conclusion=None)])],
         'is inconclusive'),
        ('job from another attempt',
         [page_document('jobs', [
             job_document(SOURCE_CHECK_JOB, 1),
             job_document(FALLBACK_JOB, 2,
                          run_attempt=int(SELF_TEST_RUN_ATTEMPT) + 1)])],
         'belongs to attempt'),
        ('job from another run',
         [page_document('jobs', [
             job_document(SOURCE_CHECK_JOB, 1),
             job_document(FALLBACK_JOB, 2, run_id=int(SELF_TEST_RUN_ID) + 1)])],
         'belongs to run'),
        ('incomplete job pagination',
         [page_document('jobs', coherent_jobs, total=3)],
         'incomplete pagination'),
        ('contradictory job pagination',
         [page_document('jobs', coherent_jobs, total=1)],
         'contradictory pagination'),
        ('job pages disagreeing on the total',
         [page_document('jobs', coherent_jobs[:1], total=2),
          page_document('jobs', coherent_jobs[1:], total=3)],
         'pages disagree on total_count'),
        ('no job pages', [], 'no API pages'),
        ('duplicate job ids',
         [page_document('jobs', [job_document(SOURCE_CHECK_JOB, 1),
                                 job_document(FALLBACK_JOB, 1)])],
         'duplicate entry id'),
    ):
        expect_check(label, validate_run_jobs, pages, SELF_TEST_RUN_ID,
                     SELF_TEST_RUN_ATTEMPT, expected=expected)

    coherent_artifacts = [
        artifact_document(ARTIFACT_NAME, 100),
        artifact_document(f'{ARTIFACT_NAME}-identity', 101),
    ]
    artifacts = validate_run_artifacts(
        [page_document('artifacts', coherent_artifacts)], SELF_TEST_RUN_ID)
    identity = {
        'artifact_id': '100',
        'artifact_digest': SELF_TEST_ARTIFACT_DIGEST,
        'run_id': SELF_TEST_RUN_ID,
        'run_attempt': SELF_TEST_RUN_ATTEMPT,
        'target': ASSET,
    }
    validate_upload_identity(identity, SELF_TEST_RUN_ID,
                             SELF_TEST_RUN_ATTEMPT, artifacts[ARTIFACT_NAME])
    for label, pages, expected in (
        ('missing candidate artifact',
         [page_document('artifacts', coherent_artifacts[1:])],
         'missing required artifact'),
        ('unexpected production artifact',
         [page_document('artifacts', coherent_artifacts + [
             artifact_document('release-assets', 102)])],
         'unexpected artifact'),
        ('candidate artifact without a supported digest',
         [page_document('artifacts', [
             artifact_document(ARTIFACT_NAME, 100, digest='not-a-digest'),
             coherent_artifacts[1]])],
         'has no supported digest'),
        ('expired candidate artifact',
         [page_document('artifacts', [
             artifact_document(ARTIFACT_NAME, 100, expired=True),
             coherent_artifacts[1]])],
         'expired'),
        ('candidate artifact from another run',
         [page_document('artifacts', [
             artifact_document(ARTIFACT_NAME, 100,
                               workflow_run={'id': int(SELF_TEST_RUN_ID) + 1}),
             coherent_artifacts[1]])],
         'belongs to another run'),
        ('incomplete artifact pagination',
         [page_document('artifacts', coherent_artifacts, total=3)],
         'incomplete pagination'),
        ('contradictory artifact pagination',
         [page_document('artifacts', coherent_artifacts, total=1)],
         'contradictory pagination'),
        ('no artifact pages', [], 'no API pages'),
        ('duplicate artifact names',
         [page_document('artifacts', coherent_artifacts + [
             artifact_document(ARTIFACT_NAME, 103)])],
         'duplicate artifact'),
    ):
        expect_check(label, validate_run_artifacts, pages, SELF_TEST_RUN_ID,
                     expected=expected)
    for label, document, artifact, expected in (
        ('identity for another artifact id',
         dict(identity, artifact_id='999'), artifacts[ARTIFACT_NAME],
         'artifact id'),
        ('identity for another digest',
         dict(identity, artifact_digest='sha256:' + '0' * 64),
         artifacts[ARTIFACT_NAME], 'artifact digest does not match'),
        ('identity for another run', dict(identity, run_id='999'),
         artifacts[ARTIFACT_NAME], 'run_id is'),
        ('identity for another attempt', dict(identity, run_attempt='2'),
         artifacts[ARTIFACT_NAME], 'run_attempt is'),
        ('identity for another target',
         dict(identity, target='aarch64-apple-darwin'),
         artifacts[ARTIFACT_NAME], 'target is'),
        ('identity with extra fields', dict(identity, extra='x'),
         artifacts[ARTIFACT_NAME], 'fields differ'),
        ('identity with a malformed digest',
         dict(identity, artifact_digest='not-a-digest'),
         artifacts[ARTIFACT_NAME], 'not a supported digest'),
    ):
        expect_check(label, validate_upload_identity, document,
                     SELF_TEST_RUN_ID, SELF_TEST_RUN_ATTEMPT, artifact,
                     expected=expected)
    expect_check('non-object upload identity', validate_upload_identity, [],
                 SELF_TEST_RUN_ID, SELF_TEST_RUN_ATTEMPT,
                 artifacts[ARTIFACT_NAME], expected='expected a JSON object')

    manifest = manifest_fixture()
    validate_manifest_binding(manifest, manifest_expectation())
    if validate_tag_binding(200, tag_ref_body(), None, manifest) != SELF_TEST_SHA:
        fail('self-test: a lightweight tag must peel to the manifest source')
    annotated_sha = 'f' * 40
    if validate_tag_binding(
            200, tag_ref_body(kind='tag', sha=annotated_sha),
            tag_object_body(), manifest) != SELF_TEST_SHA:
        fail('self-test: an annotated tag must peel to the manifest source')
    missing_field = manifest_fixture()
    missing_field.pop('source_sha')
    for label, document, expected in (
        ('manifest with an extra field', dict(manifest, extra='x'), 'unexpected'),
        ('manifest without the source', missing_field, 'missing'),
        ('manifest for another version', manifest_fixture(version='9.9.9'),
         'release manifest version is'),
        ('manifest for another target',
         manifest_fixture(target='aarch64-apple-darwin'),
         'release manifest target is'),
        ('manifest for another run', manifest_fixture(run_id='999'),
         'release manifest run_id is'),
        ('manifest for another attempt', manifest_fixture(run_attempt='2'),
         'release manifest run_attempt is'),
        ('manifest from another event',
         manifest_fixture(event='workflow_dispatch'),
         'release manifest event is'),
        ('manifest with a moved tag ref',
         manifest_fixture(original_ref='refs/tags/v9.9.9'),
         'release manifest original_ref is'),
        ('manifest with a malformed source', manifest_fixture(source_sha='x'),
         'source_sha is not a full commit SHA'),
        ('manifest with a malformed archive hash',
         manifest_fixture(archive_sha256='x'),
         'archive_sha256 is not a SHA-256'),
        ('manifest with a malformed executable hash',
         manifest_fixture(executable_sha256='x'),
         'executable_sha256 is not a SHA-256'),
        ('manifest with other lock hashes',
         manifest_fixture(lock_hashes={'Cargo.lock': '0' * 64}),
         'lock_hashes differ'),
        ('manifest with another toolchain',
         manifest_fixture(toolchain={'rust': '1.0.0'}),
         'toolchain differs'),
    ):
        expect_check(label, validate_manifest_binding, document,
                     manifest_expectation(), expected=expected)
    expect_check('non-object release manifest', validate_manifest_binding, [],
                 manifest_expectation(), expected='expected a JSON object')

    for label, status, ref_body, tag_body, document, expected in (
        ('missing release tag', 404, b'{}', None, manifest, 'is missing'),
        ('release tag lookup failure', 500, b'{}', None, manifest,
         'returned HTTP 500'),
        ('moved lightweight tag', 200,
         tag_ref_body(sha=SELF_TEST_OTHER_SHA), None, manifest,
         'not the manifest source'),
        ('annotated tag without peeling', 200,
         tag_ref_body(kind='tag', sha=annotated_sha), None, manifest,
         'was not peeled'),
        ('moved annotated tag', 200,
         tag_ref_body(kind='tag', sha=annotated_sha),
         tag_object_body(commit=SELF_TEST_OTHER_SHA), manifest,
         'not the manifest source'),
        ('annotated tag with another name', 200,
         tag_ref_body(kind='tag', sha=annotated_sha),
         tag_object_body(tag='v9.9.9'), manifest, 'is not'),
        ('tag ref with another name', 200,
         tag_ref_body(ref='refs/tags/v9.9.9'), None, manifest,
         'release tag ref is'),
        ('tag pointing at an unsupported object', 200,
         tag_ref_body(kind='tree', sha=SELF_TEST_SHA), None, manifest,
         'unsupported'),
        # Taking the source from the default-branch payload SHA instead of the
        # original tag is exactly the misuse this binding refuses.
        ('manifest source taken from the payload default-branch SHA', 200,
         tag_ref_body(), None,
         manifest_fixture(source_sha=SELF_TEST_OTHER_SHA),
         'not the manifest source'),
    ):
        expect_check(label, validate_tag_binding, status, ref_body, tag_body,
                     document, expected=expected)

    # The strict data entrypoint is what a privileged consumer calls: it wires
    # every check together over already-fetched API data. Both tag shapes work,
    # and the honest bundle form is accepted through the CLI boundary.
    def binding(*, run=None, jobs=None, artifacts_pages=None,
                identity_document=None, manifest_document=None,
                trusted=None, tag_status=200, tag_body=None,
                tag_object=None, repository=SELF_TEST_REPOSITORY):
        return validate_publication_binding(
            run or run_binding_fixture(),
            jobs or [page_document('jobs', coherent_jobs)],
            artifacts_pages or [page_document('artifacts', coherent_artifacts)],
            identity_document or identity,
            manifest_document or manifest,
            trusted or manifest_expectation(),
            repository, SELF_TEST_RUN_ID, SELF_TEST_RUN_ATTEMPT,
            SELF_TEST_WORKFLOW_ID, tag_status, tag_body or tag_ref_body(),
            tag_object)
    bound = binding()
    if bound['source_sha'] != SELF_TEST_SHA or \
            bound['artifacts'][ARTIFACT_NAME]['id'] != 100 or \
            [job['name'] for job in bound['jobs']] != list(MANDATORY_RUN_JOBS):
        fail('self-test: the publication binding must return the validated data')
    if binding(tag_body=tag_ref_body(kind='tag', sha=annotated_sha),
               tag_object=tag_object_body())['source_sha'] != SELF_TEST_SHA:
        fail('self-test: the publication binding must accept an annotated tag')
    for label, options, expected in (
        ('binding with a foreign repository',
         {'repository': 'other/repo'}, 'run binding repository is'),
        ('binding with a skipped mandatory job',
         {'jobs': [page_document('jobs', [
             job_document(SOURCE_CHECK_JOB, 1),
             job_document(FALLBACK_JOB, 2, conclusion='skipped')])]},
         'not success'),
        ('binding with a replaced artifact digest',
         {'artifacts_pages': [page_document('artifacts', [
             artifact_document(ARTIFACT_NAME, 100,
                               digest='sha256:' + '0' * 64),
             coherent_artifacts[1]])]},
         'artifact digest does not match'),
        ('binding with other lock hashes',
         {'manifest_document': manifest_fixture(
             lock_hashes={'Cargo.lock': '0' * 64})},
         'lock_hashes differ'),
        ('binding with a moved tag',
         {'tag_body': tag_ref_body(sha=SELF_TEST_OTHER_SHA)},
         'not the manifest source'),
        ('binding with a missing tag', {'tag_status': 404}, 'is missing'),
        ('binding with an incomplete job page',
         {'jobs': [page_document('jobs', coherent_jobs, total=3)]},
         'incomplete pagination'),
    ):
        expect_check(label, lambda options=options: binding(**options),
                     expected=expected)

    with tempfile.TemporaryDirectory(prefix='magit-lsp-binding-bundle-') as temporary:
        bundle = {
            'repository': SELF_TEST_REPOSITORY,
            'run_id': SELF_TEST_RUN_ID,
            'run_attempt': SELF_TEST_RUN_ATTEMPT,
            'workflow_id': SELF_TEST_WORKFLOW_ID,
            'run': run_binding_fixture(),
            'job_pages': [page_document('jobs', coherent_jobs)],
            'artifact_pages': [page_document('artifacts', coherent_artifacts)],
            'identity': identity,
            'manifest': manifest,
            'trusted': manifest_expectation(),
            'tag_status': 200,
            'tag_ref': json.loads(tag_ref_body()),
            'tag_object': None,
        }
        bundle_path = Path(temporary) / 'binding.json'
        bundle_path.write_text(json.dumps(bundle), encoding='utf-8')
        if validate_publication_binding_bundle(bundle_path)['source_sha'] != \
                SELF_TEST_SHA:
            fail('self-test: the binding bundle must return the source commit')
        bundle['tag_status'] = 404
        bundle_path.write_text(json.dumps(bundle), encoding='utf-8')
        expect_check('binding bundle with a missing tag',
                     validate_publication_binding_bundle, bundle_path,
                     expected='is missing')

    if release_target(404, b'{"message":"Not Found"}', SELF_TEST_SHA) != SELF_TEST_SHA:
        fail('self-test: an absent version tag must target the verified source commit')
    if release_target(200, tag_document(SELF_TEST_SHA), SELF_TEST_SHA) != SELF_TEST_SHA:
        fail('self-test: a version tag at the verified commit must be accepted')
    for label, status, body, expected in (
        ('same version, other commit', 200, tag_document(SELF_TEST_OTHER_SHA),
         'not the verified source'),
        ('tag lookup failure', 500, b'{}', 'returned HTTP 500'),
        ('tag lookup without a sha', 200, b'{}', 'missing commit sha'),
        ('tag lookup with invalid JSON', 200, b'not json', 'invalid JSON'),
    ):
        expect_check(label, release_target, status, body, SELF_TEST_SHA,
                     expected=expected)

    with tempfile.TemporaryDirectory(prefix='magit-lsp-release-self-test-') as temporary:
        directory = Path(temporary)
        artifacts = directory / 'dist'
        write_artifacts(artifacts)
        validate_artifacts(artifacts, '1.2.3')
        (artifacts / 'unexpected.txt').write_text('stray', encoding='utf-8')
        expect_check('extra artifact file', validate_artifacts, artifacts, '1.2.3',
                     expected='unexpected files')
        (artifacts / 'unexpected.txt').unlink()
        sidecar = next(artifacts.glob('*.sha256'))
        sidecar.write_text('0' * 64 + '  ' + next(artifacts.glob('*.tar.gz')).name + '\n',
                           encoding='utf-8')
        expect_check('tampered artifact checksum', validate_artifacts, artifacts, '1.2.3',
                     expected='sha256 mismatch')
        sidecar.unlink()
        expect_check('missing artifact sidecar', validate_artifacts, artifacts, '1.2.3',
                     expected='missing')

        # The downloaded archive must still hash to the run-bound manifest.
        archive = next((artifacts).glob('*.tar.gz'))
        bound_manifest = manifest_fixture(
            archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest())
        validate_archive_binding(bound_manifest, archive)
        expect_check('replaced release archive', validate_archive_binding,
                     manifest_fixture(), archive,
                     expected='does not match the release manifest hash')
        expect_check('missing release archive', validate_archive_binding,
                     bound_manifest, directory / 'missing.tar.gz',
                     expected='is missing')

        check_workflow()
        for label, mutations, expected in (
            ('publish workflow without a pinned validate checkout',
             (('          ref: ${{ github.sha }}\n', '', 1),),
             'validate job must check out the dispatch source commit'),
            ('publish workflow without a pinned publish checkout',
             (('          ref: ${{ github.sha }}\n\n'
               '      - uses: actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131'
               ' # v7.0.0\n        with:\n'
               '          name: validated-release-assets\n',
               '      - uses: actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131'
               ' # v7.0.0\n        with:\n'
               '          name: validated-release-assets\n', None),),
             'publish job must check out the dispatch source commit'),
            ('publish workflow without run provenance',
             ((RUN_PROVENANCE, 'python3 scripts/other.py', None),),
             'verify the selected build run provenance'),
            ('publish workflow using the VSIX before provenance',
             (('      # Fail closed before any of the selected run',
               '      - run: make vscode-package\n'
               '      # Fail closed before any of the selected run', None),),
             'provenance verification must precede'),
            ('publish workflow handing off a dist glob',
             (('            dist/magit-lsp-v${{ env.RELEASE_VERSION }}-x86_64-unknown-linux-gnu'
               '.tar.gz\n', '            dist/*\n', None),),
             'must not upload'),
            ('publish workflow uploading arbitrary release assets',
             (('"$stem" "$stem.sha256"', 'release-assets/dist/*', None),),
             'upload exactly the validated assets'),
            ('publish workflow releasing without a target',
             ((' --target "$RELEASE_TARGET"', '', None),),
             'must target the verified source commit'),
            ('publish workflow without a tag target check',
             ((TAG_TARGET_CHECK, 'python3 scripts/other.py', None),),
             'resolve the version tag target'),
            ('publish workflow without artifact re-verification',
             (('--validate-artifacts release-assets/dist', '--print-version', None),),
             're-verify the handed-off artifact set'),
            ('publish workflow reading a stored marketplace PAT',
             (('      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n',
               '      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n'
               '      VSCE_PAT: ${{ secrets.VSCE_PAT }}\n', 1),),
             'must not read secrets.VSCE_PAT'),
            ('publish workflow publishing the marketplace VSIX with a stored token',
             (('            --oidc\n', '', None),),
             'must authenticate with OIDC'),
            ('publish workflow without frozen CLI dependencies',
             (('      - name: Install frozen VS Code CLI dependencies\n'
               '        run: make vscode-deps V=1\n', '', None),),
             'frozen VSCode CLI dependencies'),
        ):
            expect_check(label, check_workflow,
                         mutated_workflow(directory, label, mutations),
                         expected=expected)

        # The build workflow keeps one pinned pipeline handoff: fixed
        # toolchain, version producer, shared build and shared artifact check.
        for label, mutations, expected in (
            ('build workflow with an unpinned build',
             ((PIPELINE_BUILD, 'cargo build --release', None),),
             'shared pinned build'),
            ('build workflow without the artifact pipeline',
             ((PIPELINE_ARTIFACTS, 'true', None),),
             'shared artifact check'),
            ('build workflow without the pinned toolchain',
             ((f'rustup toolchain install {RELEASE_TOOLCHAIN}',
               'rustup toolchain install stable', None),),
             'pinned Rust'),
            ('build workflow without the version producer',
             (('--validate-release-ref', '--print-version', None),),
             'validate the version/tag ref'),
            ('build workflow running the local installed-compiler mode',
             ((PIPELINE_BUILD, PIPELINE_BUILD_LOCAL, None),),
             'never the local installed-compiler mode'),
        ):
            expect_check(label, check_build_workflow,
                         mutated_file(BUILD_WORKFLOW, directory, label, mutations),
                         expected=expected)

        # The build workflow keeps the strict fallback, a mandatory shared
        # source-check precondition, explicit run-bound handoffs and an isolated
        # experimental native matrix. Each fixture below proves one guard still
        # fails closed after the workflow reshape.
        for label, mutations, expected in (
            ('build workflow on a mutable runner',
             (('runs-on: ubuntu-22.04', 'runs-on: ubuntu-latest', 1),),
             'mutable runner'),
            ('build workflow without the mandatory source-check job',
             (('  source-checks:', '  source-checks-disabled:', 1),),
             "missing mandatory job 'source-checks'"),
            ('build workflow producer without the source-check dependency',
             (('    needs: source-checks\n    runs-on: ubuntu-22.04',
               '    runs-on: ubuntu-22.04', 1),),
             'must need source-checks'),
            ('build workflow granting a write permission',
             (('    needs: source-checks\n    runs-on: ubuntu-22.04',
               '    needs: source-checks\n    permissions:\n'
               '      contents: write\n    runs-on: ubuntu-22.04', 1),),
             'not read-only'),
            ('build workflow with an unlocked source-check invocation',
             (('make format-check hygiene test-rust test-lua V=1',
               'make format-check hygiene test-rust V=1', 1),),
             'make test-lua'),
            ('build workflow source checks invoking the release producer',
             (('make format-check hygiene test-rust test-lua V=1',
               'make format-check hygiene test-rust test-lua V=1 release-locked', 1),),
             'must not invoke the release producer'),
            ('build workflow inferring the handoff source from another run',
             (('          RELEASE_SOURCE_SHA: ${{ github.sha }}',
               '          RELEASE_SOURCE_SHA: ${{ github.event.workflow_run.head_sha }}',
               1),),
             'RELEASE_SOURCE_SHA'),
            ('build workflow packaging the fallback for another target',
             (('          RELEASE_BUILD_TARGET: x86_64-unknown-linux-gnu',
               '          RELEASE_BUILD_TARGET: aarch64-apple-darwin', 1),),
             'packaging target is'),
            ('build workflow uploading a mutable archive glob',
             (('            build/release/artifacts/magit-lsp-v${{ env.RELEASE_VERSION }}'
               '-x86_64-unknown-linux-gnu.tar.gz\n',
               '            build/release/artifacts/*\n', 1),),
             'never a glob'),
            ('build workflow omitting the run manifest upload',
             (('            build/release/release-manifest.json\n',
               '            build/release/provenance.json\n', 1),),
             'release-manifest.json'),
            ('build workflow omitting the immutable artifact id',
             (('          ARTIFACT_ID: ${{ steps.upload.outputs.artifact-id }}',
               '          ARTIFACT_ID: fixture-id', 1),),
             'artifact-id'),
            ('build workflow omitting the immutable artifact digest',
             (('          ARTIFACT_DIGEST: ${{ steps.upload.outputs.artifact-digest }}',
               '          ARTIFACT_DIGEST: fixture-digest', 1),),
             'artifact-digest'),
            ('build workflow omitting the run attempt evidence',
             (('          RUN_ATTEMPT: ${{ github.run_attempt }}',
               '          RUN_ATTEMPT: fixture-attempt', 1),),
             'github.run_attempt'),
            ('build workflow omitting the upload identity file',
             (('          path: build/release/upload-identity.json',
               '          path: build/release/provenance.json', 1),),
             'upload-identity.json'),
            ('build workflow renaming the fallback candidate the publisher consumes',
             (('          name: linux-x86_64-gnu-release-candidate',
               '          name: release-candidate', 1),),
             'expected \'linux-x86_64-gnu-release-candidate\''),
            ('build workflow consuming a candidate by mutable name',
             (('    needs: source-checks\n    runs-on: ubuntu-22.04\n    steps:\n',
               '    needs: source-checks\n    runs-on: ubuntu-22.04\n    steps:\n'
               '      - uses: actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131'
               ' # v7.0.0\n        with:\n'
               '          name: linux-x86_64-gnu-release-candidate\n', 1),),
             'mutable name'),
            ('build workflow without the tag-only trigger filter',
             (("    tags: ['v*']", "    tags: ['release-*']", 1),),
             'push trigger must filter'),
            ('build workflow without the manual rehearsal trigger',
             (('  workflow_dispatch:\n', '', 1),),
             'workflow_dispatch plus push tags'),
            ('build workflow trying to publish',
             (('make format-check hygiene test-rust test-lua V=1',
               'make format-check hygiene test-rust test-lua V=1 && cargo publish --locked',
               1),),
             'publication step'),
            ('build workflow moving the fallback off the Ubuntu 22.04 baseline',
             (('runs-on: ubuntu-22.04', 'runs-on: ubuntu-24.04', 1),),
             'strict Linux fallback must run on ubuntu-22.04'),
            ('build workflow consuming python3 before setup-python',
             (('      # The release checker needs tomllib, so select a supported interpreter\n',
               '      - run: python3 -V\n\n'
               '      # The release checker needs tomllib, so select a supported interpreter\n',
               1),),
             'must put actions/setup-python before'),
            ('build workflow selecting an old Python interpreter',
             (("          python-version: '3.12'", "          python-version: '3.10'", 1),),
             'below the required'),
            ('build workflow with an incomplete native builder matrix',
             (('          - os: macos-26-intel\n'
               '            target: x86_64-apple-darwin\n'
               '            artifact: native-macos-intel-experimental\n', '', 1),),
             'native builder matrix'),
            ('build workflow packaging a native target for the wrong target',
             (('          RELEASE_BUILD_TARGET: ${{ matrix.target }}',
               '          RELEASE_BUILD_TARGET: x86_64-unknown-linux-gnu', 1),),
             'native candidates packaging target is'),
            ('build workflow promoting an experimental native candidate',
             (('    continue-on-error: true', '    continue-on-error: false', 1),),
             'non-gating'),
            ('build workflow naming a native candidate as production',
             (('            artifact: native-macos-arm64-experimental',
               '            artifact: macos-arm64-release-candidate', 1),),
             'experimental artifact'),
            ('build workflow building a native candidate outside its locked shell',
             (('.#release-native', '.#release-naive', None),),
             '.#release-native'),
            ('build workflow native candidate skipping the shared pipeline',
             (('nix develop --no-update-lock-file .#release-native -c python3 '
               'scripts/check_release_pipeline.py artifacts',
               'nix develop --no-update-lock-file .#release-native -c true', 1),),
             'shared pipeline'),
        ):
            expect_check(label, check_build_workflow,
                         mutated_file(BUILD_WORKFLOW, directory, label, mutations),
                         expected=expected)

        # The shared Make targets must run that same script, must not offer a
        # local compiler alias and the automation unit must stay Cargo-free.
        for label, mutations in (
            ('Makefile without the pinned build target',
             ((PIPELINE_BUILD, 'cargo build --release', None),)),
            ('Makefile without the artifact target',
             ((PIPELINE_ARTIFACTS, 'true', None),)),
            ('Makefile reintroducing the local build target',
             (('release-locked:\n',
               'release-local:\n'
               '\tpython3 scripts/check_release_pipeline.py build --local\n'
               'release-locked:\n', 1),)),
        ):
            expect_check(label, check_local_pipeline,
                         mutated_file(MAKEFILE, directory, label, mutations),
                         expected='Makefile:')
        expect_check(
            'Makefile with a Cargo automation unit', check_automation_unit,
            mutated_file(MAKEFILE, directory, 'Makefile with a Cargo automation unit',
                         ((PIPELINE_AUTOMATION_UNIT,
                           f'{PIPELINE_AUTOMATION_UNIT} && cargo metadata', None),)),
            expected=RELEASE_AUTOMATION_UNIT)

        # The strict pin is the only release build. Its locked GNU target
        # command is unchanged and a fixture toolchain that refuses `+1.94.1`
        # is still rejected; no installed-compiler path remains.
        toolchain = write_fake_toolchain(directory)
        pipeline = load_pipeline()
        expected_pinned = ['cargo', '+1.94.1', 'build', '--release', '--locked',
                           '--target', ASSET]
        if pipeline.build_command(['cargo', '+1.94.1']) != expected_pinned:
            fail('self-test: the strict pinned fallback build command changed')
        expect_pipeline_error(pipeline, 'installed compiler satisfying the CI pin',
                              pipeline.compiler, toolchain,
                              expected='pinned release build needs')
        for removed in ('compiler_for', 'installed_compiler', 'installed_prefix'):
            if hasattr(pipeline, removed):
                fail(f'self-test: the pipeline still exposes {removed}')

        # Native-linker evidence must belong to the packaged source and target;
        # a fallback candidate or another commit never adopts it.
        evidence_path = directory / 'native-build.json'
        evidence_path.write_text(json.dumps(
            {'source_sha': SELF_TEST_SHA, 'target': ASSET}), encoding='utf-8')
        evidence_identity = {'source_sha': SELF_TEST_SHA, 'target': ASSET}
        pipeline.verify_native_evidence(evidence_identity, evidence_path)
        expect_pipeline_error(
            pipeline, 'native evidence for another source',
            pipeline.verify_native_evidence,
            {'source_sha': SELF_TEST_OTHER_SHA, 'target': ASSET}, evidence_path,
            expected='does not match the package source')
        expect_pipeline_error(
            pipeline, 'native evidence for another target',
            pipeline.verify_native_evidence,
            {'source_sha': SELF_TEST_SHA, 'target': 'aarch64-apple-darwin'},
            evidence_path, expected='does not match the package target')

        # The shared script keeps the locked flags and refuses a local alias.
        for label, mutations, expected in (
            ('pipeline without --locked',
             (("'--locked', ", '', None),), 'build command is'),
            ('pipeline reintroducing a local build alias',
             (("    parser.add_argument('--target', default='',",
               "    parser.add_argument('--local', action='store_true')\n"
               "    parser.add_argument('--target', default='',", 1),),
             'permissive --local'),
        ):
            problems = pipeline_violations(
                load_pipeline(mutated_file(PIPELINE, directory, label, mutations)),
                toolchain)
            if not any(expected in problem for problem in problems):
                fail(f'self-test: {label} was accepted')

        # A missing or mismatched RELEASE_VERSION handoff must fail closed.
        pipeline = load_pipeline()
        if pipeline.resolve_version({}) != release_versions():
            fail('self-test: the local pipeline must fall back to the manifest version')
        expect_pipeline_error(pipeline, 'mismatched release version handoff',
                              pipeline.resolve_version, {'RELEASE_VERSION': '99.0.0'},
                              expected='does not match')
        expect_pipeline_error(pipeline, 'missing CI release version handoff',
                              pipeline.resolve_version, {'GITHUB_ACTIONS': 'true'},
                              expected='RELEASE_VERSION is missing')
    print('ok: release provenance, tag, artifact and workflow wiring self-tests')


def run_automation_unit():
    """Run the static workflow/pipeline guards and fixtures without Cargo.

    This is the honest no-build boundary: it excludes ``cargo metadata`` and
    ``cargo package`` so it can run in a tools-only shell. The broad
    ``release-automation-check`` remains the hosted Cargo-aware target.
    """
    run_self_tests()
    check_workflow()
    check_build_workflow()
    check_release_pipeline()
    check_local_pipeline()
    check_automation_unit()
    check_artifact_consistency()
    print('ok: release automation unit guards (no Cargo metadata or package)')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--validate-artifacts', metavar='DIR',
                        help='verify the downloaded release archive and sidecar')
    parser.add_argument('--validate-run-metadata', action='store_true',
                        help='verify the selected build run is this repository\'s '
                             'successful build of $GITHUB_SHA')
    parser.add_argument('--check-release-target', action='store_true',
                        help='reject a version tag at another commit and print '
                             'RELEASE_TARGET=<verified sha>')
    parser.add_argument('--validate-release-ref', action='store_true',
                        help='validate RELEASE_REF and the version pair, writing $GITHUB_ENV')
    parser.add_argument('--print-version', action='store_true',
                        help='print RELEASE_VERSION=<X.Y.Z> for $GITHUB_ENV')
    parser.add_argument('--pipeline-self-tests', action='store_true',
                        help='run only the release pipeline adapter self-tests '
                             '(fake processes; no Cargo, build or metadata)')
    parser.add_argument('--automation-unit', action='store_true',
                        help='run the static/self-test guards without Cargo '
                             'metadata or package')
    parser.add_argument('--validate-publication-binding', metavar='BUNDLE',
                        help='validate an offline JSON run/tag/asset binding '
                             'bundle and print its source commit')
    args = parser.parse_args()
    try:
        if args.pipeline_self_tests:
            run_pipeline_self_tests()
            return 0
        if args.automation_unit:
            run_automation_unit()
            return 0
        if args.validate_publication_binding:
            validate_publication_binding_bundle(args.validate_publication_binding)
            return 0
        if args.validate_artifacts:
            validate_artifacts(args.validate_artifacts)
            return 0
        if args.validate_run_metadata:
            validate_build_run()
            return 0
        if args.check_release_target:
            print(f'RELEASE_TARGET={check_release_target()}')
            return 0
        if args.validate_release_ref:
            validate_release_ref()
            return 0
        if args.print_version:
            print(f'RELEASE_VERSION={release_versions()}')
            return 0
        run_self_tests()
        check_workflow()
        check_build_workflow()
        check_release_pipeline()
        check_local_pipeline()
        check_artifact_consistency()
        check_metadata()
    except CheckError as error:
        print(f'FAILED: {error}', file=sys.stderr)
        return 1
    print('ok: release build/publish workflow guards and Cargo package metadata')
    return 0


if __name__ == '__main__':
    sys.exit(main())
