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
import base64
import hashlib
import inspect
import json
import os
import re
import shutil
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
# Node and pnpm come from the flake's locked dev shell, and the publish job
# resolves vsce/ovsx from the extension's locked dev dependencies, so the frozen
# install inside that shell must precede every publishing step.
NIX_DEVELOP = 'nix develop --no-update-lock-file -c'
DEPENDENCY_INSTALL = NIX_DEVELOP + ' make vscode-deps'
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
NATIVE_CONSUMER_JOB = 'native-consumers'
NATIVE_FUNCTIONAL_JOB = 'native-functional-consumers'
NATIVE_VSCODE_JOB = 'native-vscode-consumers'
FALLBACK_TARGET = ASSET
SOURCE_CHECK_TARGETS = ('format-check', 'hygiene', 'test-rust', 'test-lua')
NATIVE_MATRIX = (
    ('ubuntu-22.04', 'x86_64-unknown-linux-gnu'),
    ('macos-26', 'aarch64-apple-darwin'),
    ('macos-26-intel', 'x86_64-apple-darwin'),
)
# Each required build job runs exactly two server suites on one source, toolchain
# and workspace cache: debug before the release build, release against the exact
# packaged executable. Every other acceptance suite runs once per platform.
SERVER_DEBUG_STEP = 'Debug server tests'
SERVER_RELEASE_STEP = 'Release server tests against the exact release executable'
SERVER_SUITE = 'server_async'
NATIVE_SHELL = 'nix develop --no-update-lock-file .#release-native -c'
COVERAGE_TOKENS = ('cargo llvm-cov', 'coverage-rust', '--instrument-coverage',
                   'LLVM_PROFILE_FILE')
# Client/ABI/package acceptance runs once in the consumer jobs, never alongside
# the duplicated server suites.
ACCEPTANCE_COMMANDS = ('check_native_artifact.py', 'native-vscode-artifact-check',
                       'native-neovim-artifact-check')
RELEASE_MANIFEST = 'build/release/release-manifest.json'
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
# The single platform-independent VSIX producer and the installed-extension
# acceptance of its exact uploaded bytes. No step may rebuild or repackage it.
VSIX_JOB = 'vsix'
VSIX_ARTIFACT = 'vscode-vsix-candidate'
VSIX_IDENTITY_ARTIFACT = f'{VSIX_ARTIFACT}-identity'
VSIX_PATH = 'build/vscode/magit-lsp-${{ env.RELEASE_VERSION }}.vsix'
VSIX_MANIFEST = 'build/vscode/vsix-manifest.json'
# Publication binding: the tracked producer's mandatory jobs. The stable
# aggregates are consumed by job id, so this never infers an API matrix row name
# (GitHub appends the matrix values to job names). `source-checks` and the strict
# Linux GNU build are named for their own integrity, and the two aggregates
# already cover the macOS rows, the VSIX producer and its installed consumers.
REQUIRED_VSIX_GATE_JOB = 'required-vsix-gate'
RELEASE_READINESS_JOB = 'release-readiness'
MANDATORY_RUN_JOBS = (SOURCE_CHECK_JOB, FALLBACK_JOB, VSIX_JOB,
                      REQUIRED_VSIX_GATE_JOB, RELEASE_READINESS_JOB)
# Readiness never includes or depends on a Windows job; those stay informational.
REQUIRED_VSIX_GATE_NEEDS = (VSIX_JOB, NATIVE_VSCODE_JOB)
READINESS_NEEDS = (SOURCE_CHECK_JOB, VSIX_JOB, FALLBACK_JOB, NATIVE_JOB,
                   NATIVE_CONSUMER_JOB, NATIVE_FUNCTIONAL_JOB, NATIVE_VSCODE_JOB,
                   REQUIRED_VSIX_GATE_JOB)
REQUIRED_RUN_ARTIFACTS = (ARTIFACT_NAME, f'{ARTIFACT_NAME}-identity',
                          VSIX_ARTIFACT, VSIX_IDENTITY_ARTIFACT)
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
# The lockfiles the producer hashes into a release manifest. Publication reads
# them at the bound source commit, never from the publisher checkout.
SOURCE_LOCK_FILES = (
    'Cargo.lock',
    'flake.lock',
    'clients/vscode/pnpm-lock.yaml',
)
# Publication binding: the tracked producer's mandatory jobs. The stable

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
    """Require the fail-closed workflow_run publication boundary."""
    lines = read_lines(path)
    text = '\n'.join(lines)
    code = code_lines(lines)
    if not lines or lines[0].strip() != 'name: Publish release artifacts':
        fail('workflow: expected the publish workflow name on the first line')
    try:
        on_index = lines.index('on:')
    except ValueError:
        fail('workflow: missing top-level `on:`')
    trigger_block = block_after(lines, on_index, 0)
    triggers = [line.strip()[:-1] for line in trigger_block
                if indent_of(line) == 2 and line.strip().endswith(':')]
    if triggers != ['workflow_run'] or 'workflow_dispatch' in text:
        fail(f'workflow: trigger must be workflow_run only, found {triggers}')
    for required in ('      - Release artifact build (no publication)',
                     '    types: [completed]',
                     'python3 scripts/check_release_automation.py --prepare-publication-runtime "$GITHUB_EVENT_PATH"'):
        if required not in text:
            fail(f'workflow: missing trusted collector contract {required!r}')
    # The publisher may not rebuild, repackage, install a PAT or run a registry/
    # GitHub-release command inline: it downloads its own validated handoff and
    # hands those exact bytes to the declared make interface.
    for forbidden in ('build_run_id', 'inputs.mode', 'github.sha', 'make vscode-package',
                      'vsce package', 'pnpm run', 'cargo build', 'CARGO_REGISTRY_TOKEN',
                      'secrets.', 'gh release create', 'vsce publish',
                      'ovsx publish', 'cargo publish', 'VSCE_PAT', 'OVSX_PAT'):
        if forbidden in text:
            fail(f'workflow: forbidden legacy/bypass path {forbidden!r}')
    top_permissions = block_after(lines, find_line(lines, 'permissions:', 'workflow') - 1, 0)
    if '  contents: read' not in top_permissions:
        fail('workflow: top-level permissions must be `contents: read`')
    check_pinned_actions(lines, 'workflow')
    check_inline_scripts(lines, 'workflow')
    for forbidden in UNTRACKED_PATHS:
        if forbidden in text:
            fail(f'workflow: references operator-owned path {forbidden!r}')
    validate = require_job(lines, 'validate')
    publish = require_job(lines, 'publish')
    validate_permissions = block_after(validate, validate.index('    permissions:'), 4)
    if '      contents: read' not in validate_permissions or '      actions: read' not in validate_permissions:
        fail('workflow: validate must be read-only with actions: read')
    publish_permissions = block_after(publish, publish.index('    permissions:'), 4)
    if '      contents: write' not in publish_permissions or '      id-token: write' not in publish_permissions:
        fail('workflow: publish must scope contents/id-token write to its job')
    if text.count('contents: write') != 1 or text.count('id-token: write') != 1:
        fail('workflow: write permissions must occur only in publish')
    validate_text = '\n'.join(validate)
    if 'ref: ${{ github.event.repository.default_branch }}' not in validate_text:
        fail('workflow: validate must check out the protected default branch')
    if '--prepare-publication-runtime' not in validate_text:
        fail('workflow: validate must invoke the fail-closed runtime collector')
    if '    needs: validate' not in publish:
        fail('workflow: publish must run only after the validate job')
    guard = next((line.strip() for line in publish
                  if line.strip().startswith('if:')), None)
    if guard is None or '${{ false }}' in guard:
        fail('workflow: publish must gate on a successful push-tag build of this '
             'repository, not a disabled or unconditional job')
    for requirement in (
            "github.event.workflow_run.event == 'push'",
            "github.event.workflow_run.conclusion == 'success'",
            'github.event.workflow_run.head_repository.full_name == github.repository'):
        if requirement not in guard:
            fail(f'workflow: publish guard must require {requirement!r}')
    if '    environment: release-publish' not in publish:
        fail('workflow: publish job must require release-publish environment')
    # The read-only validate job uploads only the runtime's own handoff directory
    # and exposes its immutable upload id; the protected publisher downloads that
    # exact id, installs the frozen locked tooling inside the flake's dev shell
    # and calls the declared make interface. A candidate by name, an inline
    # registry command, a rebuild or a PAT never satisfies this contract.
    for required in (
            '        name: publication-handoff',
            "      handoff_artifact_id: ${{ steps.handoff-upload.outputs.artifact-id }}",
            '        uses: actions/download-artifact@',
            '          artifact-ids: ${{ needs.validate.outputs.handoff_artifact_id }}',
            "          node-version: '24'",
            f'        run: {DEPENDENCY_INSTALL}',
            '        run: make release-publish HANDOFF=build/release/publication-handoff/publication-handoff.json',
    ):
        if required not in text:
            fail(f'workflow: missing immutable handoff/publication contract {required!r}')
    if 'actions/download-artifact@' in validate_text:
        fail('workflow: only the protected publish job may download the handoff')
    publish_text = '\n'.join(publish)
    if '          name: publication-handoff' in publish_text:
        fail('workflow: the handoff download must use the immutable artifact id, '
             'not a mutable name')
    if '          path: build/release/publication-handoff' not in publish_text:
        fail('workflow: the handoff download must target the declared handoff path')
    deps_line = next((index for index, line in enumerate(publish)
                      if line.strip() == f'run: {DEPENDENCY_INSTALL}'), None)
    publish_line = next((index for index, line in enumerate(publish)
                         if line.strip().startswith('run: make release-publish')),
                        None)
    if deps_line is None or publish_line is None or deps_line > publish_line:
        fail('workflow: frozen publication tooling must be installed before publishing')


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


def check_native_promotion_evidence(selected_artifact, evidence):
    """Fail closed if production selects a native artifact without complete hosted proof."""
    if 'experimental' not in selected_artifact:
        return
    if not isinstance(evidence, dict):
        fail('build workflow: native promotion requires complete identity-bound hosted evidence')
    identity = evidence.get('identity')
    if not isinstance(identity, dict) or not all(identity.get(key) for key in
            ('source_sha', 'run_url', 'run_attempt')):
        fail('build workflow: native promotion evidence is missing source/run/attempt identity')
    artifacts = evidence.get('artifacts')
    if not isinstance(artifacts, dict):
        fail('build workflow: native promotion evidence is missing immutable artifacts')
    for _, target in NATIVE_MATRIX:
        artifact = artifacts.get(target)
        if not isinstance(artifact, dict) or not artifact.get('immutable_id') or \
                not re.fullmatch(r'[0-9a-f]{64}', str(artifact.get('sha256', ''))):
            fail(f'build workflow: native promotion evidence lacks immutable ID/hash for {target}')
        if any(artifact.get(key) != identity.get(key)
               for key in ('source_sha', 'run_url', 'run_attempt')):
            fail(f'build workflow: native promotion artifact identity mismatch for {target}')
    outcomes = evidence.get('outcomes')
    required_gates = ('abi', 'functional', 'vsix')
    if not isinstance(outcomes, dict):
        fail('build workflow: native promotion evidence is missing mandatory outcomes')
    for gate in required_gates:
        results = outcomes.get(gate)
        if not isinstance(results, dict):
            fail(f'build workflow: native promotion evidence is missing {gate} outcomes')
        for _, target in NATIVE_MATRIX:
            if results.get(target) != 'success':
                fail(f'build workflow: native promotion {gate} outcome for {target} must be success')


def check_native_candidates(block):
    """Require the strict, mandatory Linux GNU/macOS matrix experiment.

    Every required row is a real job: ``continue-on-error`` would mask a failed
    or cancelled row, so it is refused outright. The rows keep their
    ``experimental`` artifact labels; production promotion is not claimed here.
    """
    code = '\n'.join(code_lines(block))
    if not any(line.strip() == 'needs: source-checks' for line in block):
        fail('build workflow: the native producers must need source-checks')
    if 'continue-on-error:' in code:
        fail('build workflow: the required native matrix must stay mandatory; '
             'continue-on-error would mask a failed Linux/macOS row')
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
    if 'env -u NIX_CFLAGS_COMPILE -u NIX_LDFLAGS make release-native-build' not in code:
        fail('build workflow: native candidates must clear the dev shell Nix flag '
             'variables before the native build')
    if PIPELINE_ARTIFACTS not in code:
        fail('build workflow: native candidates must package through the shared pipeline')
    check_producer_handoff(block, '${{ matrix.target }}', 'native candidates', True)


def check_native_consumers(block):
    """Require clean native hosts to consume only their bound immutable candidates."""
    code = '\n'.join(code_lines(block))
    if not any(line.strip() == 'needs: native-candidates' for line in block):
        fail('build workflow: native consumers must need native-candidates')
    if 'continue-on-error:' in code:
        fail('build workflow: native consumers must be mandatory')
    matrix = {}
    artifacts = {}
    for index, line in enumerate(block):
        match = re.match(r'^\s*- os:\s*(\S+)\s*$', line)
        if not match:
            continue
        target = artifact = None
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
    if matrix != dict(NATIVE_MATRIX):
        fail('build workflow: native consumer matrix must exactly match all three native hosts')
    expected_artifacts = dict(zip((host for host, _ in NATIVE_MATRIX), (
        'native-linux-x86_64-gnu-experimental',
        'native-macos-arm64-experimental',
        'native-macos-intel-experimental')))
    if artifacts != expected_artifacts:
        fail('build workflow: native consumers must select each exact experimental candidate')
    if 'cachix/install-nix-action' in code or 'nix develop' in code:
        fail('build workflow: native consumers must run on clean hosts without Nix')
    for forbidden in ('cargo ', 'rustup ', 'release-native-build', 'make '):
        if forbidden in code:
            fail(f'build workflow: native consumers must not build or install tools ({forbidden.strip()})')
    required = (
        'permissions:', 'actions: read', 'contents: read',
        'GITHUB_TOKEN: ${{ github.token }}', 'actions/setup-python@',
        "python-version: '3.12'", 'python3 -m scripts.download_native_candidate',
        '--run-id "${{ github.run_id }}"', '--attempt "${{ github.run_attempt }}"',
        '--source-sha "${{ github.sha }}"', '--target "${{ matrix.target }}"',
        '--artifact-name "${{ matrix.artifact }}"',
        'scripts/check_native_artifact.py --functional "$archive"', 'if: always()',
        'native-candidate.json', 'native-evidence.json', 'if-no-files-found: error',
    )
    for token in required:
        if token not in code:
            fail(f'build workflow: native consumers must include {token!r}')
    if 'persist-credentials: false' not in code or 'ref: ${{ github.sha }}' not in code:
        fail('build workflow: native consumers must use immutable credential-free source checkout')
    if 'test "${{ steps.candidate.outcome }}" = success' not in code:
        fail('build workflow: native consumer candidate/check failures must fail the job')


def check_native_functional_consumers(block):
    """Require separate, mandatory artifact-backed native functional suites."""
    code = '\n'.join(code_lines(block))
    if not any(line.strip() == 'needs: native-candidates' for line in block):
        fail('build workflow: native functional consumers must need native-candidates')
    if 'continue-on-error:' in code:
        fail('build workflow: native functional consumers must be mandatory')
    matrix = {}
    for index, line in enumerate(block):
        match = re.match(r'^\s*- os:\s*(\S+)\s*$', line)
        if not match:
            continue
        target = None
        for entry in block[index + 1:]:
            if entry.strip() and indent_of(entry) <= indent_of(line):
                break
            found = re.match(r'^\s*target:\s*(\S+)\s*$', entry)
            if found:
                target = found.group(1)
        matrix[match.group(1)] = target
    if matrix != dict(NATIVE_MATRIX):
        fail('build workflow: native functional matrix must exactly match all three native hosts')
    for forbidden in ('release-native-build', 'cargo build', 'cargo test --release', 'continue-on-error:'):
        if forbidden in code:
            fail(f'build workflow: functional consumers must not substitute a server build ({forbidden})')
    required = (
        'needs: native-candidates', 'permissions:', 'actions: read', 'contents: read',
        'cachix/install-nix-action@', '.#release-acceptance',
        'python3 -m scripts.download_native_candidate', '--run-id "${{ github.run_id }}"',
        '--attempt "${{ github.run_attempt }}"', '--source-sha "${{ github.sha }}"',
        '--target "${{ matrix.target }}"', '--artifact-name "${{ matrix.artifact }}"',
        'cargo test --no-run --test server_async --test editor_bridge --test rebase_interactive',
        'test-rust-artifact',
        'FILTER="--test server_async --test editor_bridge --test rebase_interactive"',
        'native-neovim-artifact-check', 'MAGIT_LSP_TEST_BINARY=',
        'executable_sha256', 'before_sha256', 'after_sha256',
        'native-functional-evidence.json', 'if: always()', 'if-no-files-found: warn',
        'steps.harness.outcome', 'steps.rust-functional.outcome',
    )
    for token in required:
        if token not in code:
            fail(f'build workflow: native functional consumers must include {token!r}')
    if 'persist-credentials: false' not in code or 'ref: ${{ github.sha }}' not in code:
        fail('build workflow: functional consumers must use immutable credential-free checkout')
    if 'test "$after" = "$expected"' not in code:
        fail('build workflow: functional candidate hash must remain unchanged after runs')
    if "grep -E '([0-9]+ ignored|skipped)'" not in code:
        fail('build workflow: functional suites must fail on ignored or skipped cases')
    if 'test "${{ steps.candidate.outcome }}" = success' not in code:
        fail('build workflow: functional candidate/harness/runtime failures must fail the job')


def check_native_vscode_consumers(block):
    """Require installed-VSIX acceptance on each immutable native candidate host."""
    code = '\n'.join(code_lines(block))
    if 'continue-on-error:' in code or any(line.startswith('    if:') for line in block):
        fail('build workflow: native VSCode consumers must be mandatory')
    if 'needs: [native-candidates, vsix]' not in code:
        fail('build workflow: native VSCode consumers must need native and VSIX producers')
    matrix = {}
    for index, line in enumerate(block):
        match = re.match(r'^\s*- os:\s*(\S+)\s*$', line)
        if match:
            target = None
            for entry in block[index + 1:]:
                if entry.strip() and indent_of(entry) <= indent_of(line):
                    break
                found = re.match(r'^\s*target:\s*(\S+)\s*$', entry)
                if found:
                    target = found.group(1)
            matrix[match.group(1)] = target
    if matrix != dict(NATIVE_MATRIX):
        fail('build workflow: native VSCode matrix must exactly match all three native hosts')
    required = ('python3 -m scripts.download_native_candidate',
                'python3 -m scripts.download_vsix_candidate',
                '--run-id "${{ github.run_id }}"', '--attempt "${{ github.run_attempt }}"',
                '--source-sha "${{ github.sha }}"', '--ref "${{ github.ref }}"',
                '--event "${{ github.event_name }}"',
                'from scripts.check_native_artifact import safe_extract',
                'make native-vscode-artifact-check', 'MAGIT_LSP_SERVER_PATH=',
                'RELEASE_ARTIFACT=', 'RELEASE_MANIFEST=', 'RELEASE_VSIX=',
                'native-candidate.json', 'vsix-candidate.json',
                'native_before', 'native_after', 'vsix_before', 'vsix_after',
                'test "$native_before" = "$native_after"',
                'test "$vsix_before" = "$vsix_after"',
                'xvfb-run -a make native-vscode-artifact-check',
                'if: always()', 'native-vscode-evidence.json', 'if-no-files-found: warn')
    for token in required:
        if token not in code:
            fail(f'build workflow: native VSCode consumers must include {token!r}')
    if 'persist-credentials: false' not in code or 'ref: ${{ github.sha }}' not in code:
        fail('build workflow: native VSCode consumers must use immutable source checkout')
    for forbidden in ('vscode-package', 'vscode-deps', 'cargo build', 'release-native-build'):
        if forbidden in code:
            fail(f'build workflow: native VSCode consumers must not build or package ({forbidden})')
    if "if: runner.os == 'Linux'" not in code or "if: runner.os == 'macOS'" not in code:
        fail('build workflow: editor setup must cover Linux and macOS hosted runners')


def check_consumer_entrypoints(path=BUILD_WORKFLOW):
    """Prove the consumer Python entrypoints on a clean runner, without network.

    ``python3 -m`` resolves a repository-root module from the working directory
    even when ``PYTHONPATH`` is unset; a script path does not add the repository
    root to ``sys.path``, so these subprocesses fail exactly where the workflow
    would. The installed-VSIX step must import the one real extractor from its
    owning module instead of an aliased or duplicated downloader symbol.
    """
    code = '\n'.join(code_lines(read_lines(path)))
    for script in ('scripts/download_native_candidate.py',
                   'scripts/download_vsix_candidate.py'):
        if script in code:
            fail(f'build workflow: consumer steps must invoke the root module, not '
                 f'the script path {script!r}')
    for module in ('scripts.download_native_candidate',
                   'scripts.download_vsix_candidate'):
        if f'python3 -m {module}' not in code:
            fail(f'build workflow: consumer steps must invoke `python3 -m {module}`')
    if 'from scripts.check_native_artifact import safe_extract' not in code:
        fail('build workflow: installed-VSIX extraction must import the owning '
             "`from scripts.check_native_artifact import safe_extract`")
    if 'from scripts.download_native_candidate import safe_extract' in code:
        fail('build workflow: installed-VSIX extraction must not alias safe_extract '
             'from the downloader module')
    root = Path(__file__).resolve().parents[1]
    owners = [script for script in sorted((root / 'scripts').glob('*.py'))
              if re.search(r'^def safe_extract\b',
                           script.read_text(encoding='utf-8'), re.MULTILINE)]
    if owners != [root / 'scripts/check_native_artifact.py']:
        found = ', '.join(str(script.relative_to(root)) for script in owners) or 'none'
        fail('build workflow: safe_extract must be defined once in '
             f'scripts/check_native_artifact.py, found {found}')
    environment = {key: value for key, value in os.environ.items() if key != 'PYTHONPATH'}
    commands = [[sys.executable, '-m', module, '--help']
                for module in ('scripts.download_native_candidate',
                               'scripts.download_vsix_candidate')]
    commands.append([sys.executable, '-c',
                     'from scripts.check_native_artifact import safe_extract'])
    for command in commands:
        result = subprocess.run(command, cwd=root, env=environment,
                                capture_output=True, text=True)
        if result.returncode != 0:
            detail = (result.stderr or result.stdout).strip().splitlines()
            tail = detail[-6:] if detail else ['no output']
            fail(f'consumer entrypoint {" ".join(command)} exited '
                 f'{result.returncode}: ' + ' | '.join(tail))


def job_needs(block, context):
    """Return the declared ``needs:`` job ids of one workflow job block."""
    for index, line in enumerate(block):
        if line.strip().startswith('needs:'):
            value = line.split(':', 1)[1].strip()
            if value.startswith('[') and value.endswith(']'):
                return [item.strip() for item in value[1:-1].split(',') if item.strip()]
            if value:
                return [value]
            needs = []
            for entry in block[index + 1:]:
                if entry.strip() and indent_of(entry) <= indent_of(line):
                    break
                if entry.strip().startswith('- '):
                    needs.append(entry.strip()[2:].strip())
            return needs
    return []


def check_aggregate_gate(block, name, expected_needs):
    """Require one fail-closed aggregate over exactly the expected job ids.

    ``always()`` plus an explicit ``needs.<job>.result`` check for every
    dependency means an absent, failed, skipped or cancelled required job blocks
    the aggregate instead of being silently omitted. The gate runs no build,
    package or install step, so it never rebuilds the bytes it gates.
    """
    if not block:
        fail(f'build workflow: missing required aggregate job {name!r}')
    code = '\n'.join(code_lines(block))
    needs = job_needs(block, name)
    if tuple(needs) != tuple(expected_needs):
        fail(f'build workflow: {name} must need exactly {list(expected_needs)!r}, '
             f'found {needs!r}')
    if 'continue-on-error:' in code:
        fail(f'build workflow: {name} must be mandatory')
    if not any(line.strip() == 'if: always()' for line in block):
        fail(f'build workflow: {name} must run with `if: always()` so a missing, '
             'failed, skipped or cancelled dependency still evaluates the gate')
    if 'uses:' in code:
        fail(f'build workflow: {name} must not run an action or build step')
    for forbidden in ('checkout@', 'make ', 'vsce ', 'npm ', 'pnpm ', 'cargo ',
                      'setup-node'):
        if forbidden in code:
            fail(f'build workflow: {name} must not build, install or package '
                 f'({forbidden.strip()})')
    windows = [line.strip() for line in code_lines(block)
               if re.match(r'^\s*(?:-\s*)?(?:os|runs-on):\s*windows', line,
                           re.IGNORECASE)]
    if windows:
        fail(f'build workflow: {name} must never gate on Windows ({windows[0]!r})')
    for need in expected_needs:
        variable = need.upper().replace('-', '_') + '_RESULT'
        if f'{variable}: ${{{{ needs.{need}.result }}}}' not in code:
            fail(f'build workflow: {name} must read needs.{need}.result')
        if f'check {need} "${variable}"' not in code:
            fail(f'build workflow: {name} must fail unless {need} succeeded')
    # The presence of a check call is not enough: the helper itself must compare
    # against success and exit non-zero, or a skipped/cancelled result could ride
    # along as a successful aggregate.
    if 'check() { if [ "$2" != success ]; then' not in code or \
            'exit 1; fi; }' not in code:
        fail(f'build workflow: {name} must exit non-zero for any required result '
             'other than success')


def job_steps(block):
    """Split a job block into its steps, each as ``(name, lines)``.

    Steps are the entries at the job's six-space sequence indent; the name is
    the step's ``- name:`` value or ``''`` for an unnamed ``uses`` step.
    """
    steps = []
    for line in block:
        if re.match(r'^      - ', line):
            name = re.search(r'^      - name:\s*(.+?)\s*$', line)
            steps.append([name.group(1) if name else '', [line]])
        elif steps:
            steps[-1][1].append(line)
    return [(name, lines) for name, lines in steps]


def single_step(steps, predicate, label, context):
    """Return the index of the one step whose content matches ``predicate``."""
    matches = [index for index, (_, lines) in enumerate(steps)
               if predicate('\n'.join(lines))]
    if len(matches) != 1:
        fail(f'build workflow: {context} must run exactly one {label} step in the '
             f'same job, found {len(matches)}')
    return matches[0]


def check_server_test_graph(context, block, target, native):
    """Require one same-job debug-before-build and release-after-package pair.

    The server suite is the only suite repeated, it runs against the exact
    packaged executable recorded by the run-bound manifest, and no instrumented
    coverage or consumer acceptance may mix into either run.
    """
    steps = job_steps(block)
    for token in ACCEPTANCE_COMMANDS:
        if token in '\n'.join(code_lines(block)):
            fail(f'build workflow: {context} must not repeat client/ABI/package '
                 'acceptance in the build job')
    debug = single_step(steps, lambda text: 'make test-rust FILTER' in text,
                        'debug server test', context)
    build = single_step(
        steps, lambda text: PIPELINE_BUILD in text or 'make release-native-build' in text,
        'release build', context)
    package = single_step(steps, lambda text: PIPELINE_ARTIFACTS in text,
                          'packaging', context)
    release = single_step(steps, lambda text: 'make test-rust-artifact' in text,
                          'release server test', context)
    if not debug < build < package < release:
        fail(f'build workflow: {context} must run debug server tests before the '
             'release build and release server tests after packaging')
    if steps[debug][0] != SERVER_DEBUG_STEP:
        fail(f'build workflow: {context} debug server tests must run in the '
             f'{SERVER_DEBUG_STEP!r} step')
    if steps[release][0] != SERVER_RELEASE_STEP:
        fail(f'build workflow: {context} release server tests must run in the '
             f'{SERVER_RELEASE_STEP!r} step')
    debug_text = '\n'.join(steps[debug][1])
    release_text = '\n'.join(steps[release][1])
    if 'MAGIT_LSP_TEST_BINARY' in debug_text:
        fail(f'build workflow: {context} debug server tests must not target the '
             'release executable')
    acceptance = ACCEPTANCE_COMMANDS + ('--test editor_bridge',
                                        '--test rebase_interactive')
    for token in acceptance + COVERAGE_TOKENS:
        if token in debug_text or token in release_text:
            fail(f'build workflow: {context} server tests must not repeat client/ABI/'
                 f'package acceptance or mix coverage ({token!r})')
    if 'cargo build' in release_text or 'release-native-build' in release_text:
        fail(f'build workflow: {context} release server tests must run the packaged '
             'executable, not rebuild the server')
    for label, text in (('debug', debug_text), ('release', release_text)):
        suites = re.findall(r'--test\s+([\w-]+)', text)
        if suites != [SERVER_SUITE]:
            fail(f'build workflow: {context} {label} server tests may run only '
                 f'{SERVER_SUITE!r}, found {suites}')
        if native and NATIVE_SHELL not in text:
            fail(f'build workflow: {context} {label} server tests must use the same '
                 'locked .#release-native shell as the build')
        if not native and 'nix develop' in text:
            fail(f'build workflow: {context} {label} server tests must use the '
                 'same pinned rustup toolchain as the build')
    if '--locked' not in debug_text:
        fail(f'build workflow: {context} debug server tests must run the locked '
             'server suite before the build')
    expected_binary = f'target/{target}/release/magit-lsp'
    if 'realpath' not in release_text or expected_binary not in release_text:
        fail(f'build workflow: {context} release server tests must run the exact '
             f'packaged executable {expected_binary!r}')
    if RELEASE_MANIFEST not in release_text or 'executable_sha256' not in release_text:
        fail(f'build workflow: {context} release server tests must bind '
             f'{RELEASE_MANIFEST} executable_sha256')
    digest = 'shasum -a 256' if native else 'sha256sum'
    if digest not in release_text or '= "$expected"' not in release_text \
            or 'MAGIT_LSP_TEST_BINARY="$binary"' not in release_text:
        fail(f'build workflow: {context} release server tests must compare the '
             f'packaged binary hash before running it')


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
    for required in (SOURCE_CHECK_JOB, FALLBACK_JOB, NATIVE_JOB, NATIVE_CONSUMER_JOB,
                     NATIVE_FUNCTIONAL_JOB, NATIVE_VSCODE_JOB):
        if required not in jobs:
            fail(f'build workflow: missing mandatory job {required!r}')
    require_pinned_builders(jobs)

    check_vsix_producer(jobs.get(VSIX_JOB, []))

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
    for alternate in ('cargo build', 'package-release.sh'):
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
    selected_name, _ = step_uploads(upload_steps(fallback)[0], 'fallback')
    # This is the production artifact selection point. No hosted proof is checked
    # in locally; selecting an experimental artifact therefore fails closed.
    check_native_promotion_evidence(selected_name, None)
    fallback_name = check_producer_handoff(fallback, FALLBACK_TARGET, 'fallback', False)
    if 'experimental' in fallback_name:
        fail('build workflow: experimental native artifacts cannot be selected as production')
    if fallback_name != ARTIFACT_NAME:
        fail(f'build workflow: uploads {fallback_name!r}, expected {ARTIFACT_NAME!r} '
             'so the publisher consumes the strict fallback')
    check_server_test_graph(FALLBACK_JOB, fallback, FALLBACK_TARGET, False)
    if any(line.strip() == 'continue-on-error: true' for line in fallback):
        fail('build workflow: the strict Linux GNU fallback must remain mandatory')

    # Windows may stay informational in this repository, but it can never gate a
    # required release build or acceptance job.
    required_jobs = (SOURCE_CHECK_JOB, FALLBACK_JOB, NATIVE_JOB, NATIVE_CONSUMER_JOB,
                     NATIVE_FUNCTIONAL_JOB, NATIVE_VSCODE_JOB, VSIX_JOB,
                     REQUIRED_VSIX_GATE_JOB, RELEASE_READINESS_JOB)
    for job_name, block in jobs.items():
        windows = [line.strip() for line in code_lines(block)
                   if re.match(r'^\s*(?:-\s*)?(?:os|runs-on):\s*windows', line,
                               re.IGNORECASE)]
        if not windows:
            continue
        if job_name in required_jobs or 'continue-on-error: true' not in \
                '\n'.join(code_lines(block)):
            fail('build workflow: Windows cannot gate the required release graph '
                 f'({windows[0]!r} in {job_name!r})')
    for token in COVERAGE_TOKENS:
        if token in text:
            fail('build workflow: instrumented coverage belongs in the separate '
                 f'read-only CI coverage job ({token!r})')

    check_native_candidates(jobs[NATIVE_JOB])
    check_server_test_graph(NATIVE_JOB, jobs[NATIVE_JOB], '${{ matrix.target }}',
                            True)
    check_native_consumers(jobs[NATIVE_CONSUMER_JOB])
    check_native_functional_consumers(jobs[NATIVE_FUNCTIONAL_JOB])
    check_native_vscode_consumers(jobs[NATIVE_VSCODE_JOB])
    # The two stable aggregates are the release-readiness contract: the VSIX
    # gate covers the producer and its installed consumers, and readiness covers
    # every required Linux/macOS source, build and acceptance job. Windows is
    # absent from both graphs.
    check_aggregate_gate(jobs.get(REQUIRED_VSIX_GATE_JOB), REQUIRED_VSIX_GATE_JOB,
                         REQUIRED_VSIX_GATE_NEEDS)
    check_aggregate_gate(jobs.get(RELEASE_READINESS_JOB), RELEASE_READINESS_JOB,
                         READINESS_NEEDS)
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


def check_vsix_producer(block):
    """Require one platform-independent, run-bound VSIX producer and upload."""
    if not block:
        fail('build workflow: missing VSIX producer job')
    code = '\n'.join(code_lines(block))
    runner = next((line.split(':', 1)[1].strip() for line in block
                   if line.strip().startswith('runs-on:')), '')
    if runner != 'ubuntu-24.04' or 'matrix:' in code:
        fail('build workflow: VSIX producer must be platform-independent')
    if 'needs: source-checks' not in code:
        fail('build workflow: VSIX producer must need source-checks')
    if NIX_DEVELOP not in code:
        fail('build workflow: VSIX producer must package inside the flake dev shell')
    if 'uses: actions/setup-node@' in code or 'pnpm@' in code:
        fail('build workflow: VSIX producer must take Node and pnpm from the '
             'flake dev shell, not runner-provided tooling')
    makefile = '\n'.join(read_lines(MAKEFILE))
    if 'make vscode-package' not in code or 'vscode-package:' not in makefile \
            or '--frozen-lockfile' not in makefile:
        fail('build workflow: VSIX producer must use the frozen existing package target')
    if sum(1 for step in upload_steps(block)
           if VSIX_PATH in '\n'.join(step_uploads(step, 'VSIX producer')[1])) != 1:
        fail('build workflow: VSIX producer must upload exactly one VSIX')
    if 'make vscode-package' not in code:
        fail('build workflow: VSIX producer must package the VSIX')
    if 'cargo build' in code or 'release-native-build' in code:
        fail('build workflow: VSIX producer must not rebuild the native server')
    if VSIX_MANIFEST not in code or 'artifact-digest' not in code \
            or 'artifact-id' not in code:
        fail('build workflow: VSIX producer must hand off manifest and immutable artifact identity')
    provenance = job_step(block, '--write-vsix-binding', 'VSIX producer')
    environment = step_environment(provenance, 'VSIX producer')
    expected_identity = {
        'RELEASE_SOURCE_SHA': '${{ github.sha }}',
        'RELEASE_REF': '${{ github.ref }}',
        'RELEASE_EVENT': '${{ github.event_name }}',
        'RELEASE_RUN_ID': '${{ github.run_id }}',
        'RELEASE_RUN_ATTEMPT': '${{ github.run_attempt }}',
    }
    if any(environment.get(key) != value for key, value in expected_identity.items()):
        fail('build workflow: VSIX producer must bind source/ref/event/run/attempt')
    if 'contents: write' in code or 'secrets.' in code:
        fail('build workflow: VSIX producer must have no publication rights')


def write_vsix_binding():
    """Write hash and run identity for the exact VSIX produced by vscode-package."""
    version = os.environ.get('RELEASE_VERSION', '')
    if VERSION_PATTERN.fullmatch(version) is None:
        fail('VSIX provenance: invalid RELEASE_VERSION')
    package = json.loads(EXTENSION_MANIFEST.read_text(encoding='utf-8'))
    if package.get('version') != version:
        fail('VSIX provenance: package version drift')
    artifact = Path(f'build/vscode/magit-lsp-{version}.vsix')
    if not artifact.is_file():
        fail('VSIX provenance: expected exactly one versioned VSIX')
    if list(Path('build/vscode').glob('*.vsix')) != [artifact]:
        fail('VSIX provenance: missing or extra VSIX package')
    locks = ('Cargo.lock', 'flake.lock', 'clients/vscode/pnpm-lock.yaml')
    payload = {
        'source_sha': os.environ.get('RELEASE_SOURCE_SHA'),
        'original_ref': os.environ.get('RELEASE_REF'),
        'event': os.environ.get('RELEASE_EVENT'),
        'run_id': os.environ.get('RELEASE_RUN_ID'),
        'run_attempt': os.environ.get('RELEASE_RUN_ATTEMPT'),
        'version': version,
        'vsix_sha256': hashlib.sha256(artifact.read_bytes()).hexdigest(),
        'lock_hashes': {name: hashlib.sha256(Path(name).read_bytes()).hexdigest()
                        for name in locks},
        'pnpm_version': '10.13.1',
    }
    if not all(payload[key] for key in ('source_sha', 'original_ref', 'event',
                                        'run_id', 'run_attempt')):
        fail('VSIX provenance: missing run identity')
    if payload['source_sha'] != os.environ.get('GITHUB_SHA'):
        fail('VSIX provenance: source SHA does not match the workflow run')
    if payload['run_id'] != os.environ.get('GITHUB_RUN_ID') or \
            payload['run_attempt'] != os.environ.get('GITHUB_RUN_ATTEMPT'):
        fail('VSIX provenance: run ID or attempt does not match the workflow run')
    Path('build/vscode/vsix-manifest.json').write_text(
        json.dumps(payload, sort_keys=True) + '\n', encoding='utf-8')


def write_vsix_upload_identity():
    """Record the immutable upload ID and digest for the VSIX handoff."""
    fields = ('ARTIFACT_ID', 'ARTIFACT_DIGEST', 'RUN_ID', 'RUN_ATTEMPT')
    payload = {key.lower(): os.environ.get(key, '') for key in fields}
    if not all(payload.values()) or not ARTIFACT_DIGEST_PATTERN.fullmatch(
            payload['artifact_digest']):
        fail('VSIX upload identity: missing or invalid immutable upload identity')
    Path('build/vscode/vsix-upload-identity.json').write_text(
        json.dumps(payload, sort_keys=True) + '\n', encoding='utf-8')


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
        ('release-publish',
         'python3 scripts/publish_release.py --handoff "$(HANDOFF)"'),
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
    """The producer uploads candidates; the publisher transfers only its handoff."""
    build_jobs = workflow_jobs(read_lines(BUILD_WORKFLOW))
    uploaded = step_artifact_name(
        build_jobs[FALLBACK_JOB], 'actions/upload-artifact@', 'build workflow')
    if uploaded != ARTIFACT_NAME:
        fail(f'build workflow: uploads {uploaded!r}, expected {ARTIFACT_NAME!r}')
    publish = require_job(read_lines(WORKFLOW), 'publish')
    publish_text = '\n'.join(publish)
    if 'actions/download-artifact@' not in publish_text:
        fail('publish workflow: the publisher must download its validated handoff')
    if '          artifact-ids: ${{ needs.validate.outputs.handoff_artifact_id }}' \
            not in publish_text:
        fail('publish workflow: the handoff must be addressed by the validate '
             'artifact id')
    if '          name: publication-handoff' in publish_text:
        fail('publish workflow: the handoff must never be downloaded by name')
    if 'build/release/publication-handoff' not in publish_text:
        fail('publish workflow: the handoff must use the declared handoff path')


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


def fetch_source_locks(repository, source_sha, token):
    """Return each tracked lockfile's sha256 at the immutable ``source_sha``.

    The locks are read as inert bytes through the ordinary read-only contents
    API at the bound commit, so the expectation follows the release source and
    not the publisher checkout. Unavailable or malformed data fails closed;
    nothing is checked out, executed or read from the local tree.
    """
    if not re.fullmatch(r'[0-9a-f]{40}', str(source_sha)):
        fail('release source lock lookup requires a full commit SHA')
    digests = {}
    for name in SOURCE_LOCK_FILES:
        status, body = fetch_api(
            repository, f'/contents/{name}?ref={source_sha}', token)
        if status != 200:
            fail(f'release source {name} at {source_sha} returned HTTP {status}')
        document = parse_json(body, f'release source {name}')
        if not isinstance(document, dict) or document.get('type') != 'file' \
                or document.get('path') != name:
            fail(f'release source {name} is not the tracked file')
        if document.get('encoding') != 'base64':
            fail(f'release source {name} is not base64-encoded content')
        content = document.get('content')
        if not isinstance(content, str) or not content:
            fail(f'release source {name} has no content')
        try:
            data = base64.b64decode(''.join(content.split()), validate=True)
        except ValueError:
            fail(f'release source {name} content is not valid base64')
        if isinstance(document.get('size'), bool) or document.get('size') != len(data):
            fail(f'release source {name} size differs from its content')
        digests[name] = hashlib.sha256(data).hexdigest()
    return digests


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
        fail('release manifest lock_hashes differ from the bound release source')
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
        fail('upload identity artifact digest does not match the API artifact '
             f'(identity {document["artifact_digest"]!r}, API {artifact.get("digest")!r})')


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


def publication_api_pages(repository, path, token, key, api_get=None):
    """Collect every API page for a publication listing, rejecting truncation."""
    entries = []
    if api_get is None:
        api_get = fetch_api
    total = None
    page = 1
    seen_pages = set()
    while True:
        status, body = api_get(
            repository, f'{path}?per_page=100&page={page}', token)
        if status != 200:
            fail(f'publication API {path} page {page} returned HTTP {status}')
        document = parse_json(body, f'publication API {path} page {page}')
        if not isinstance(document, dict):
            fail(f'publication API {path} page {page}: expected object')
        count = document.get('total_count')
        values = document.get(key)
        if isinstance(count, bool) or not isinstance(count, int) or count < 0 or \
                not isinstance(values, list):
            fail(f'publication API {path} page {page}: invalid listing')
        if total is None:
            total = count
        elif total != count:
            fail(f'publication API {path} pages disagree on total_count')
        page_signature = json.dumps(values, sort_keys=True, separators=(',', ':'))
        if page_signature in seen_pages:
            fail(f'publication API {path} page {page} repeats an earlier page')
        seen_pages.add(page_signature)
        previous_count = len(entries)
        entries.extend(values)
        if len(entries) >= total:
            break
        if len(entries) == previous_count:
            fail(f'publication API {path} page {page} made no pagination progress')
        page += 1
    if len(entries) != total:
        fail(f'publication API {path}: incomplete pagination')
    return [{'total_count': total, key: entries}]


def validate_publication_orchestration(event, repository, checkout_ref,
                                       checkout_sha, default_ref='refs/heads/main'):
    """Require trusted workflow_run orchestration and the protected checkout.

    Readiness is a property of the required producer jobs themselves (see
    :data:`MANDATORY_RUN_JOBS`), not of a separate readiness/promotion bundle, so
    no extra evidence schema is demanded here.
    """
    if checkout_ref != default_ref:
        fail(f'publication orchestration ref is {checkout_ref!r}, expected protected default')
    if not isinstance(checkout_sha, str) or not re.fullmatch(r'[0-9a-f]{40}', checkout_sha):
        fail('publication orchestration checkout SHA is invalid')
    if not isinstance(event, dict) or event.get('action') != 'completed':
        fail('publication event must be workflow_run completed')
    run = event.get('workflow_run')
    if not isinstance(run, dict) or full_name(run.get('repository')) != repository:
        fail('publication event run belongs to another repository')
    return run


def prepare_publication_runtime(event_path):
    """Validate the event and upstream API binding, then write the exact-byte handoff.

    The required aggregate and integrity jobs must all be completed successfully;
    the native and VSIX candidates are downloaded and validated as immutable
    artifacts without executing or rebuilding them. Registry publication is not
    enabled here.
    """
    event = parse_json(Path(event_path).read_bytes(), 'workflow_run event')
    repository = os.environ.get('GITHUB_REPOSITORY', '')
    ref = os.environ.get('GITHUB_REF', '')
    checkout_sha = os.environ.get('GITHUB_SHA', '')
    if os.environ.get('GITHUB_EVENT_NAME') != 'workflow_run':
        fail('publication orchestration requires workflow_run event')
    token = os.environ.get('GH_TOKEN', '')
    status, body = fetch_api(repository, '', token)
    if status != 200:
        fail(f'publication repository metadata returned HTTP {status}')
    repo_document = parse_json(body, 'publication repository metadata')
    default_branch = repo_document.get('default_branch') if isinstance(repo_document, dict) else None
    if not isinstance(default_branch, str) or not default_branch:
        fail('publication repository has no default branch')
    expected_ref = f'refs/heads/{default_branch}'
    if ref != expected_ref:
        fail(f'publication orchestration ref is {ref!r}, expected default {expected_ref!r}')
    # The validation job's token has only contents/actions read. "Get a branch"
    # requires Contents: read, while branch protection requires
    # Administration: read, which no workflow permission can grant. Only the
    # branch's ordinary ``protected`` metadata is checked here; detailed
    # protection/ruleset settings stay an explicit maintainer prerequisite.
    status, body = fetch_api(repository, f'/branches/{default_branch}', token)
    if status != 200:
        fail(f'publication default branch metadata returned HTTP {status}')
    branch_document = parse_json(body, 'publication default branch metadata')
    if not isinstance(branch_document, dict):
        fail('publication default branch metadata: expected object')
    if branch_document.get('protected') is not True:
        fail('publication default branch metadata does not report a protected branch')
    status, body = fetch_api(repository, f'/git/ref/heads/{default_branch}', token)
    if status != 200:
        fail(f'publication default branch ref returned HTTP {status}')
    default_ref = parse_json(body, 'publication default branch ref')
    default_object = default_ref.get('object') if isinstance(default_ref, dict) else None
    if not isinstance(default_object, dict) or default_object.get('sha') != checkout_sha:
        fail('publication orchestration checkout SHA is not the current default branch SHA')
    run = validate_publication_orchestration(
        event, repository, expected_ref, checkout_sha, default_ref=expected_ref)
    run_id = run.get('id')
    attempt = run.get('run_attempt')
    if isinstance(run_id, bool) or not isinstance(run_id, int) or run_id < 1:
        fail('publication event has invalid upstream run id')
    if isinstance(attempt, bool) or not isinstance(attempt, int) or attempt < 1:
        fail('publication event has invalid upstream run attempt')
    status, body = fetch_api(repository, f'/actions/runs/{run_id}', token)
    if status != 200:
        fail(f'publication upstream run returned HTTP {status}')
    upstream = parse_json(body, 'publication upstream run')
    if not isinstance(upstream, dict):
        fail('publication upstream run: expected object')
    workflow_id = upstream.get('workflow_id')
    if run.get('workflow_id') != workflow_id or run.get('event') != RUN_EVENT:
        fail('publication event/upstream workflow identity or event differs')
    original_ref = run.get('head_branch')
    if not isinstance(original_ref, str) or not original_ref.startswith('v'):
        fail('publication upstream run does not identify an original version tag')
    tag_ref = f'refs/tags/{original_ref}'
    validate_run_binding(upstream, repository, run_id, attempt, workflow_id,
                         tag_ref)
    # Required producer jobs are checked by stable job id: the two fail-closed
    # aggregates plus source-checks and the strict Linux GNU build. Inference of
    # API matrix row names is neither needed nor accepted, and a missing, failed,
    # skipped or cancelled required job blocks publication here.
    validate_run_jobs(
        publication_api_pages(repository, f'/actions/runs/{run_id}/jobs', token,
                              'jobs'),
        run_id, attempt)
    tag_status, tag_body = fetch_api(repository, f'/git/ref/tags/{original_ref}', token)
    tag_document = parse_json(tag_body, 'publication original tag ref') if tag_status == 200 else None
    if tag_status != 200:
        fail(f'publication original tag lookup returned HTTP {tag_status}')
    target = tag_document.get('object') if isinstance(tag_document, dict) else None
    tag_object = None
    if isinstance(target, dict) and target.get('type') == 'tag':
        object_sha = target.get('sha')
        tag_status, tag_body = fetch_api(repository, f'/git/tags/{object_sha}', token)
        if tag_status != 200:
            fail(f'publication annotated tag lookup returned HTTP {tag_status}')
        tag_object = parse_json(tag_body, 'publication annotated tag')
    if tag_object is None and isinstance(target, dict) and target.get('type') != 'commit':
        fail('publication original tag has an unsupported object type')
    # Artifact bytes are untrusted data. Authenticate only against the fixed
    # GitHub API origin, bind the identity ZIP to its immutable artifact ID,
    # then validate the candidate ZIP and its manifest before considering gates.
    if str(Path.cwd()) not in sys.path:
        sys.path.insert(0, str(Path.cwd()))
    from scripts import download_native_candidate as transport
    artifacts = validate_run_artifacts(
        publication_api_pages(repository, f'/actions/runs/{run_id}/artifacts', token,
                              'artifacts'), run_id)
    candidate = artifacts[ARTIFACT_NAME]
    identity_artifact = artifacts[f'{ARTIFACT_NAME}-identity']
    owner, _, repo = repository.partition('/')
    if not owner or not repo:
        fail('publication repository slug is invalid')

    def artifact_bytes(artifact):
        url = (f'{transport.API}/repos/{owner}/{repo}/actions/artifacts/'
               f'{artifact["id"]}/zip')
        try:
            with transport.request(url, token) as response:
                return response.read()
        except transport.DownloadError as error:
            fail(f'publication artifact download failed: {error}')

    identity_zip = artifact_bytes(identity_artifact)
    if transport.digest(identity_zip) != identity_artifact['digest'].removeprefix('sha256:'):
        fail('publication identity transport digest mismatch')
    try:
        identity_files = transport.exact_zip(identity_zip, ['upload-identity.json'])
        identity = parse_json(identity_files['upload-identity.json'], 'publication upload identity')
    except (transport.DownloadError, KeyError) as error:
        fail(f'publication identity artifact is invalid: {error}')
    validate_upload_identity(identity, run_id, attempt, candidate)
    candidate_zip = artifact_bytes(candidate)
    if transport.digest(candidate_zip) != candidate['digest'].removeprefix('sha256:'):
        fail('publication candidate transport digest mismatch')
    try:
        files = transport.exact_zip(candidate_zip)
        manifest = parse_json(files['release-manifest.json'], 'publication release manifest')
    except (transport.DownloadError, KeyError) as error:
        fail(f'publication candidate artifact is invalid: {error}')
    # Peel the manifest's original tag before trusting any lock expectation:
    # the locks belong to the immutable source commit, so they are fetched as
    # data at that SHA and never read from the publisher checkout or default
    # branch. A legitimate older tag therefore releases its own lockfiles.
    if not isinstance(manifest, dict) or not isinstance(
            manifest.get('original_ref'), str) or not isinstance(
            manifest.get('source_sha'), str):
        fail('release manifest is missing its original_ref/source_sha binding')
    tag_status, tag_body = fetch_api(repository, f'/git/ref/tags/{original_ref}', token)
    tag_object_body = None
    tag_target = tag_document.get('object') if isinstance(tag_document, dict) else None
    if isinstance(tag_target, dict) and tag_target.get('type') == 'tag':
        peel_status, peel_body = fetch_api(repository, f'/git/tags/{tag_target.get("sha")}', token)
        if peel_status != 200:
            fail(f'publication annotated tag lookup returned HTTP {peel_status}')
        tag_object_body = peel_body
    source_sha = validate_tag_binding(tag_status, tag_body, tag_object_body, manifest)
    source_locks = fetch_source_locks(repository, source_sha, token)
    trusted = {
        'run_id': str(run_id), 'run_attempt': str(attempt), 'event': RUN_EVENT,
        'version': original_ref[1:], 'target': ASSET, 'original_ref': tag_ref,
        'lock_hashes': {name: source_locks[name]
                        for name in ('Cargo.lock', 'flake.lock')},
        'toolchain': {'rust': RELEASE_TOOLCHAIN},
    }
    validate_manifest_binding(manifest, trusted)
    archive_name = f'magit-lsp-v{manifest["version"]}-{manifest["target"]}.tar.gz'
    required = {archive_name, archive_name + '.sha256', 'release-manifest.json',
                'release-manifest.json.sha256'}
    if {Path(name).name for name in files} != required:
        fail('publication candidate asset list differs from the validated manifest')
    archive = next(data for name, data in files.items() if Path(name).name == archive_name)
    checksum = next(data for name, data in files.items()
                    if Path(name).name == archive_name + '.sha256')
    manifest_bytes = files['release-manifest.json']
    try:
        sidecar = files['release-manifest.json.sha256'].decode('ascii').split()
        checksum_tokens = checksum.decode('ascii').split()
    except UnicodeDecodeError:
        fail('publication candidate checksums are not ASCII')
    if len(sidecar) != 2 or sidecar[1] != 'release-manifest.json' or \
            transport.digest(manifest_bytes) != sidecar[0] or \
            transport.digest(archive) != manifest['archive_sha256'] or \
            len(checksum_tokens) != 2 or checksum_tokens[1] != archive_name or \
            checksum_tokens[0] != transport.digest(archive):
        fail('publication candidate asset hash validation failed')
    # Persist the exact validated native bytes next to the VSIX. The artifact
    # upload carries this whole directory, so every later consumer addresses
    # the files by handoff-relative names, never by a path from this job's
    # temporary workspace. The extraction below only rechecks the executable.
    handoff_dir = Path(os.environ.get('PUBLICATION_HANDOFF_DIR') or
                       'build/release/publication-handoff')
    handoff_dir.mkdir(parents=True, exist_ok=True)
    persisted = {
        archive_name: archive,
        archive_name + '.sha256': checksum,
        'release-manifest.json': manifest_bytes,
        'release-manifest.json.sha256': files['release-manifest.json.sha256'],
    }
    for name, data in persisted.items():
        destination = handoff_dir / name
        if destination.is_symlink():
            fail(f'publication handoff {name} must not be a symbolic link')
        destination.write_bytes(data)
        if destination.is_symlink() or \
                transport.digest(destination.read_bytes()) != transport.digest(data):
            fail(f'publication handoff {name} was not persisted as exact bytes')
    with tempfile.TemporaryDirectory(prefix='publication-assets-') as temporary:
        try:
            binary = transport.native.safe_extract(
                handoff_dir / archive_name, Path(temporary) / 'extracted',
                archive_name.removesuffix('.tar.gz'))
        except Exception as error:
            fail(f'publication release archive is invalid: {error}')
        if transport.native.digest(binary) != manifest['executable_sha256']:
            fail('publication executable hash differs from manifest')
    # One VSIX is built once by the Node 24/frozen-lockfile producer. The
    # publisher downloads those exact uploaded bytes and validates them through
    # the same transport verifier the installed-extension consumers use; nothing
    # here rebuilds, repackages or reinstalls the extension.
    from scripts import download_vsix_candidate as vsix_transport
    try:
        vsix = vsix_transport.fetch_candidate(
            repository, run_id, attempt, source_sha, tag_ref, RUN_EVENT,
            original_ref[1:], handoff_dir, token, opener=transport.request,
            expected_locks=source_locks)
    except (vsix_transport.DownloadError, KeyError, ValueError, OSError) as error:
        fail(f'publication VSIX handoff is invalid: {error}')
    # The handoff records only handoff-relative names for the exact accepted
    # bytes and their immutable identities, never a rebuilt package or a
    # promotion bundle. The protected publisher consumes these paths verbatim.
    handoff = {
        'source_sha': source_sha,
        'version': manifest['version'],
        'original_ref': tag_ref,
        'event': RUN_EVENT,
        'run_id': str(run_id),
        'run_attempt': str(attempt),
        'target': ASSET,
        'native': {
            'artifact_id': str(candidate.get('id')),
            'artifact_digest': candidate.get('digest'),
            'archive': archive_name,
            'checksum': archive_name + '.sha256',
            'manifest': 'release-manifest.json',
            'manifest_checksum': 'release-manifest.json.sha256',
            'archive_sha256': manifest['archive_sha256'],
            'manifest_sha256': transport.digest(manifest_bytes),
            'executable_sha256': manifest['executable_sha256'],
        },
        'vsix': {
            'artifact_id': vsix['artifact_id'],
            'artifact_digest': vsix['artifact_digest'],
            'run_id': vsix['run_id'],
            'run_attempt': vsix['run_attempt'],
            'source_sha': vsix['source_sha'],
            # Relocatable: resolved against the published handoff JSON's directory.
            'vsix': Path(vsix['vsix']).name,
            'manifest': Path(vsix['manifest']).name,
            'vsix_sha256': vsix['vsix_sha256'],
        },
    }
    handoff_path = handoff_dir / 'publication-handoff.json'
    handoff_path.write_text(json.dumps(handoff, sort_keys=True) + '\n',
                            encoding='utf-8')
    print(f'ok: publication handoff validated for {source_sha}; '
          f'VSIX {vsix["vsix_sha256"]}; publication is delegated to the '
          'protected publisher')
    return handoff


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


def test_vsix_binding_fixtures(directory):
    """Exercise VSIX provenance generation and rejection paths offline."""
    root = Path(directory)
    (root / 'clients/vscode').mkdir(parents=True)
    (root / 'build/vscode').mkdir(parents=True)
    (root / 'clients/vscode/package.json').write_text(
        '{"version":"1.2.3"}', encoding='utf-8')
    for lock in ('Cargo.lock', 'flake.lock', 'clients/vscode/pnpm-lock.yaml'):
        (root / lock).write_text('fixture lock', encoding='utf-8')
    artifact = root / 'build/vscode/magit-lsp-1.2.3.vsix'
    artifact.write_bytes(b'fixture VSIX bytes')
    env = {
        'RELEASE_VERSION': '1.2.3', 'RELEASE_SOURCE_SHA': SELF_TEST_SHA,
        'GITHUB_SHA': SELF_TEST_SHA, 'RELEASE_REF': 'refs/tags/v1.2.3',
        'RELEASE_EVENT': 'push', 'RELEASE_RUN_ID': '1234',
        'GITHUB_RUN_ID': '1234', 'RELEASE_RUN_ATTEMPT': '2',
        'GITHUB_RUN_ATTEMPT': '2',
    }
    saved = {key: os.environ.get(key) for key in env}
    cwd = Path.cwd()
    try:
        os.environ.update(env)
        os.chdir(root)
        write_vsix_binding()
        manifest = json.loads(Path('build/vscode/vsix-manifest.json').read_text())
        if manifest['vsix_sha256'] != hashlib.sha256(artifact.read_bytes()).hexdigest():
            fail('self-test: VSIX provenance did not hash the exact fixture bytes')
        os.environ['RELEASE_SOURCE_SHA'] = SELF_TEST_OTHER_SHA
        expect_check('VSIX provenance with wrong source', write_vsix_binding,
                     expected='source SHA does not match')
        os.environ['RELEASE_SOURCE_SHA'] = SELF_TEST_SHA
        os.environ['RELEASE_RUN_ATTEMPT'] = '1'
        expect_check('VSIX provenance with wrong run attempt', write_vsix_binding,
                     expected='run ID or attempt does not match')
        os.environ['RELEASE_RUN_ATTEMPT'] = '2'
        os.environ['RELEASE_VERSION'] = '1.2.4'
        expect_check('VSIX provenance with version drift', write_vsix_binding,
                     expected='package version drift')
        os.environ['RELEASE_VERSION'] = '1.2.3'
        (Path('build/vscode') / 'unexpected.vsix').write_bytes(b'extra')
        expect_check('VSIX provenance with extra package', write_vsix_binding,
                     expected='missing or extra VSIX')
    finally:
        os.chdir(cwd)
        for key, value in saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


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


def publisher_event(**overrides):
    """Return a trusted successful push workflow_run event for the fixtures."""
    document = {'action': 'completed', 'workflow_run': {
        'id': int(SELF_TEST_RUN_ID), 'run_attempt': int(SELF_TEST_RUN_ATTEMPT),
        'event': 'push', 'conclusion': 'success',
        'head_branch': f'v{SELF_TEST_VERSION}', 'head_sha': SELF_TEST_SHA,
        'path': BUILD_WORKFLOW_PATH, 'name': BUILD_WORKFLOW_NAME,
        'repository': {'full_name': SELF_TEST_REPOSITORY},
        'head_repository': {'full_name': SELF_TEST_REPOSITORY}}}
    document.update(overrides)
    return document


def publisher_environment(**overrides):
    """Return the GitHub Actions OIDC context the publisher requires."""
    environment = {
        'GITHUB_ACTIONS': 'true', 'GITHUB_EVENT_NAME': 'workflow_run',
        'GITHUB_REPOSITORY': SELF_TEST_REPOSITORY,
        'ACTIONS_ID_TOKEN_REQUEST_URL': 'https://oidc.example.invalid/request',
        'ACTIONS_ID_TOKEN_REQUEST_TOKEN': 'fixture-oidc-request-token',
    }
    environment.update(overrides)
    return environment


def publisher_backend(module, release=None, fail=None):
    """Return an offline publisher backend that records external calls.

    ``fail`` selects one external failure: ``lookup`` before any mutation,
    ``upload`` after the created release is recorded (as ``gh release create``
    does before uploading assets), ``readback`` on the lookup that follows
    creation, or ``marketplace``/``openvsx`` in a registry lane.
    """
    class Recording:
        def __init__(self):
            self.release = None if release is None else dict(release)
            self.calls = []

        def release_assets(self, repository, tag):
            self.calls.append(('release_assets', repository, tag))
            if fail == 'lookup':
                raise module.PublishError('gh release lookup failed')
            if fail == 'readback' and any(
                    call[0] == 'release_assets' for call in self.calls[:-1]):
                raise module.PublishError('gh release readback failed')
            return None if self.release is None else dict(self.release)

        def create_release(self, repository, tag, title, assets):
            self.calls.append(('create_release', tag,
                               [Path(path).name for path in assets]))
            self.release = {
                Path(path).name: 'sha256:' + hashlib.sha256(
                    Path(path).read_bytes()).hexdigest() for path in assets}
            if fail == 'upload':
                raise module.PublishError('gh release asset upload failed')

        def publish_marketplace(self, vsix_path):
            self.calls.append(('marketplace', str(vsix_path)))
            if fail == 'marketplace':
                raise module.PublishError('vsce publish failed')

        def publish_openvsx(self, vsix_path):
            self.calls.append(('openvsx', str(vsix_path)))
            if fail == 'openvsx':
                raise module.PublishError('ovsx publish failed')

    return Recording()


def publisher_release_body(tag, assets, draft=False):
    """Return one GitHub release API body with tag identity, draft state and assets."""
    return json.dumps({
        'tag_name': tag, 'draft': draft,
        'assets': [{'name': name, 'digest': digest}
                   for name, digest in assets.items()]})


def publisher_command(responses):
    """Return an argv log and a ``subprocess.run`` stand-in for the real backend.

    ``responses`` is consumed in order as ``(returncode, stdout, stderr)``
    tuples. Driving :class:`publish_release.CommandBackend` this way exercises
    its actual ``gh``/registry command parsing and response validation offline.
    """
    calls, pending = [], list(responses)

    def run(argv, **kwargs):
        calls.append([str(token) for token in argv])
        if not pending:
            fail(f'self-test: unexpected external command {argv!r}')
        returncode, stdout, stderr = pending.pop(0)
        return subprocess.CompletedProcess(argv, returncode, stdout=stdout,
                                           stderr=stderr)

    return calls, run


def registry_commands_run(calls):
    """Return the recorded registry invocations from a publisher argv log."""
    return [call for call in calls if call[0].endswith(('vsce', 'ovsx'))]


def expect_publish_error(module, label, function, *args, expected=None,
                         required=(), forbidden=(), **kwargs):
    """Fail the self-test unless the publisher rejects the fixture."""
    try:
        function(*args, **kwargs)
    except module.PublishError as error:
        text = str(error)
        if expected is not None and expected not in text:
            fail(f'self-test: {label} failed for another reason: {error}')
        for fragment in required:
            if fragment not in text:
                fail(f'self-test: {label} omitted {fragment!r}: {error}')
        for fragment in forbidden:
            if fragment in text:
                fail(f'self-test: {label} wrongly claimed {fragment!r}: {error}')
        return
    fail(f'self-test: {label} was accepted')


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
    if str(Path.cwd()) not in sys.path:
        sys.path.insert(0, str(Path.cwd()))
    from scripts import publish_release
    # The retired readiness draft must not reappear: neither its CLI
    # entrypoints/arguments nor its collector/validator symbols may remain.
    options = set(build_parser()._option_string_actions)
    for retired in ('--collect-release-readiness', '--validate-release-readiness',
                    '--repository', '--run-id', '--attempt', '--source-sha',
                    '--ref', '--event', '--workflow-id'):
        if retired in options:
            fail(f'self-test: retired readiness entrypoint {retired} is exposed')
    for retired in ('collect_release_readiness', 'validate_release_readiness',
                    'READINESS_TARGETS', 'READINESS_OUTCOMES',
                    'PENDING_READINESS_JOBS'):
        if hasattr(sys.modules[__name__], retired):
            fail(f'self-test: retired readiness symbol {retired} remains')
    promotion_identity = {
        'source_sha': SELF_TEST_SHA,
        'run_url': 'https://github.com/example/magit-lsp/actions/runs/42',
        'run_attempt': '1',
    }
    promotion_proof = {
        'identity': promotion_identity,
        'artifacts': {target: {**promotion_identity, 'immutable_id': f'id-{index}',
                              'sha256': 'a' * 64}
                      for index, (_, target) in enumerate(NATIVE_MATRIX)},
        'outcomes': {gate: {target: 'success' for _, target in NATIVE_MATRIX}
                     for gate in ('abi', 'functional', 'vsix')},
    }
    expect_check('promotion without hosted proof', check_native_promotion_evidence,
                 'native-linux-experimental', None, expected='complete identity-bound hosted evidence')
    mismatch = {**promotion_proof, 'artifacts': dict(promotion_proof['artifacts'])}
    mismatch['artifacts'] = {**mismatch['artifacts'], 'x86_64-unknown-linux-gnu': {
        **mismatch['artifacts']['x86_64-unknown-linux-gnu'], 'source_sha': SELF_TEST_OTHER_SHA}}
    expect_check('promotion artifact identity mismatch', check_native_promotion_evidence,
                 'native-linux-experimental', mismatch, expected='artifact identity mismatch')
    for gate in ('abi', 'functional', 'vsix'):
        for _, target in NATIVE_MATRIX:
            missing = {**promotion_proof, 'outcomes': {
                key: dict(value) for key, value in promotion_proof['outcomes'].items()}}
            del missing['outcomes'][gate][target]
            expect_check(f'promotion missing {gate} outcome for {target}',
                         check_native_promotion_evidence, 'native-linux-experimental',
                         missing, expected=f'{gate} outcome for {target} must be success')
            unsuccessful = {**promotion_proof, 'outcomes': {
                key: dict(value) for key, value in promotion_proof['outcomes'].items()}}
            unsuccessful['outcomes'][gate][target] = 'failure'
            expect_check(f'promotion failed {gate} outcome for {target}',
                         check_native_promotion_evidence, 'native-linux-experimental',
                         unsuccessful, expected=f'{gate} outcome for {target} must be success')
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
    # accepted together. The native matrix stays a non-gating experiment.
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
                     job_document(FALLBACK_JOB, 2),
                     job_document(VSIX_JOB, 3),
                     job_document(REQUIRED_VSIX_GATE_JOB, 4),
                     job_document(RELEASE_READINESS_JOB, 5)]
    jobs = validate_run_jobs(
        [page_document('jobs', coherent_jobs + [
            job_document('native-candidates', 6, conclusion='skipped'),
            job_document('native-windows-experimental', 7,
                         conclusion='failure')])],
        SELF_TEST_RUN_ID, SELF_TEST_RUN_ATTEMPT)
    if [job['name'] for job in jobs][:len(MANDATORY_RUN_JOBS)] != \
            list(MANDATORY_RUN_JOBS):
        fail('self-test: coherent jobs must list the mandatory producer and '
             'aggregate jobs')
    for label, pages, expected in (
        ('missing mandatory job',
         [page_document('jobs', [job_document(SOURCE_CHECK_JOB, 1)])],
         'missing mandatory job'),
        ('missing required VSIX aggregate',
         [page_document('jobs', [job for job in coherent_jobs
                                 if job['name'] != REQUIRED_VSIX_GATE_JOB])],
         "missing mandatory job 'required-vsix-gate'"),
        ('missing release-readiness aggregate',
         [page_document('jobs', [job for job in coherent_jobs
                                 if job['name'] != RELEASE_READINESS_JOB])],
         "missing mandatory job 'release-readiness'"),
        ('failed release-readiness aggregate',
         [page_document('jobs', [
             job if job['name'] != RELEASE_READINESS_JOB
             else job_document(RELEASE_READINESS_JOB, 5, conclusion='failure')
             for job in coherent_jobs])],
         'not success'),
        ('skipped required VSIX aggregate',
         [page_document('jobs', [
             job if job['name'] != REQUIRED_VSIX_GATE_JOB
             else job_document(REQUIRED_VSIX_GATE_JOB, 4, conclusion='skipped')
             for job in coherent_jobs])],
         'not success'),
        ('cancelled release-readiness aggregate',
         [page_document('jobs', [
             job if job['name'] != RELEASE_READINESS_JOB
             else job_document(RELEASE_READINESS_JOB, 5, conclusion='cancelled')
             for job in coherent_jobs])],
         'not success'),
        ('skipped mandatory VSIX producer',
         [page_document('jobs', [
             job if job['name'] != VSIX_JOB
             else job_document(VSIX_JOB, 3, conclusion='skipped')
             for job in coherent_jobs])],
         'not success'),
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
         [page_document('jobs', coherent_jobs, total=len(coherent_jobs) + 1)],
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
        artifact_document(VSIX_ARTIFACT, 102),
        artifact_document(VSIX_IDENTITY_ARTIFACT, 103),
    ]
    artifacts = validate_run_artifacts(
        [page_document('artifacts', coherent_artifacts)], SELF_TEST_RUN_ID)
    identity = {
        'artifact_id': '100',
        'artifact_digest': artifacts[ARTIFACT_NAME]['digest'],
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
        ('missing required VSIX producer artifact',
         [page_document('artifacts', [artifact for artifact in coherent_artifacts
                                      if artifact['name'] != VSIX_ARTIFACT])],
         "missing required artifact 'vscode-vsix-candidate'"),
        ('missing required VSIX identity artifact',
         [page_document('artifacts', [artifact for artifact in coherent_artifacts
                                      if artifact['name'] != VSIX_IDENTITY_ARTIFACT])],
         "missing required artifact 'vscode-vsix-candidate-identity'"),
        ('unexpected production artifact',
         [page_document('artifacts', coherent_artifacts + [
             artifact_document('release-assets', 104)])],
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
         [page_document('artifacts', coherent_artifacts,
                        total=len(coherent_artifacts) + 1)],
         'incomplete pagination'),
        ('contradictory artifact pagination',
         [page_document('artifacts', coherent_artifacts, total=1)],
         'contradictory pagination'),
        ('no artifact pages', [], 'no API pages'),
        ('duplicate artifact names',
         [page_document('artifacts', coherent_artifacts + [
             artifact_document(ARTIFACT_NAME, 105)])],
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
             job if job['name'] != FALLBACK_JOB
             else job_document(FALLBACK_JOB, 2, conclusion='skipped')
             for job in coherent_jobs])]},
         'not success'),
        ('binding with a skipped required aggregate',
         {'jobs': [page_document('jobs', [
             job if job['name'] != RELEASE_READINESS_JOB
             else job_document(RELEASE_READINESS_JOB, 5, conclusion='skipped')
             for job in coherent_jobs])]},
         'not success'),
        ('binding with a replaced artifact digest',
         {'artifacts_pages': [page_document('artifacts', [
             artifact_document(ARTIFACT_NAME, 100,
                               digest='sha256:' + '0' * 64),
             *coherent_artifacts[1:]])]},
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
         {'jobs': [page_document('jobs', coherent_jobs,
                                 total=len(coherent_jobs) + 1)]},
         'incomplete pagination'),
    ):
        expect_check(label, lambda options=options: binding(**options),
                     expected=expected)

    orchestration_event = {
        'action': 'completed',
        'workflow_run': {'id': int(SELF_TEST_RUN_ID),
                         'run_attempt': int(SELF_TEST_RUN_ATTEMPT),
                         'repository': {'full_name': SELF_TEST_REPOSITORY},
                         'head_sha': SELF_TEST_OTHER_SHA},
    }
    if validate_publication_orchestration(
            orchestration_event, SELF_TEST_REPOSITORY,
            'refs/heads/main', SELF_TEST_SHA) != \
            orchestration_event['workflow_run']:
        fail('self-test: protected-default orchestration must remain distinct from source SHA')
    for label, event, repo, ref, sha, expected in (
        ('branch orchestration', orchestration_event, SELF_TEST_REPOSITORY,
         'refs/heads/feature', SELF_TEST_SHA, 'protected default'),
        ('foreign event repository', dict(orchestration_event, workflow_run=dict(
            orchestration_event['workflow_run'], repository={'full_name': 'other/repo'})),
         SELF_TEST_REPOSITORY, 'refs/heads/main', SELF_TEST_SHA, 'another repository'),
        ('malformed orchestration checkout SHA', orchestration_event,
         SELF_TEST_REPOSITORY, 'refs/heads/main', 'not-a-sha',
         'orchestration checkout SHA'),
    ):
        expect_check(label, validate_publication_orchestration, event, repo,
                     ref, sha, expected=expected)

    # Exercise the runtime boundary with mocked GitHub API responses: this
    # proves the collector binds the workflow_run payload to upstream API data.
    from unittest.mock import patch
    if str(Path.cwd()) not in sys.path:
        sys.path.insert(0, str(Path.cwd()))
    from scripts import download_native_candidate as runtime_transport_module
    import io
    import tarfile
    import zipfile

    def zip_bytes(entries):
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, 'w', zipfile.ZIP_DEFLATED) as archive:
            for name, value in entries.items():
                archive.writestr(name, value)
        return stream.getvalue()

    executable = b'fixture executable bytes'
    tar_stream = io.BytesIO()
    with tarfile.open(fileobj=tar_stream, mode='w:gz') as archive:
        for name, value, mode in (
                ('magit-lsp', executable, 0o755), ('LICENSE', b'license', 0o644),
                ('LICENSE-APACHE', b'apache', 0o644), ('INSTALL.md', b'install', 0o644)):
            info = tarfile.TarInfo(f'magit-lsp-v{SELF_TEST_VERSION}-{ASSET}/{name}')
            info.size, info.mode = len(value), mode
            archive.addfile(info, io.BytesIO(value))
    # The bound release source owns its lockfiles. These fixture bytes
    # deliberately differ from this publisher checkout, so a successful run
    # proves the expectation comes from the source SHA, not the publisher tree.
    source_lock_bytes = {
        name: f'fixture bound-source {name}\n'.encode('utf-8')
        for name in SOURCE_LOCK_FILES}
    source_lock_hashes = {
        name: hashlib.sha256(data).hexdigest()
        for name, data in source_lock_bytes.items()}
    for name in SOURCE_LOCK_FILES:
        if source_lock_hashes[name] == hashlib.sha256(
                Path(name).read_bytes()).hexdigest():
            fail(f'self-test: the {name} source fixture must differ from the '
                 'publisher checkout')
    runtime_manifest = manifest_fixture(
        lock_hashes={name: source_lock_hashes[name]
                     for name in ('Cargo.lock', 'flake.lock')},
        archive_sha256=hashlib.sha256(tar_stream.getvalue()).hexdigest(),
        executable_sha256=hashlib.sha256(executable).hexdigest())
    runtime_manifest_bytes = json.dumps(runtime_manifest).encode()
    runtime_archive_name = f'magit-lsp-v{SELF_TEST_VERSION}-{ASSET}.tar.gz'
    runtime_assets = {
        runtime_archive_name: tar_stream.getvalue(),
        runtime_archive_name + '.sha256':
            f'{hashlib.sha256(tar_stream.getvalue()).hexdigest()}  {runtime_archive_name}\n'.encode(),
        'release-manifest.json': runtime_manifest_bytes,
        'release-manifest.json.sha256':
            f'{hashlib.sha256(runtime_manifest_bytes).hexdigest()}  release-manifest.json\n'.encode(),
    }
    runtime_candidate_zip = zip_bytes(runtime_assets)
    runtime_identity = {
        'artifact_id': '100', 'artifact_digest': 'sha256:' + hashlib.sha256(runtime_candidate_zip).hexdigest(),
        'run_id': SELF_TEST_RUN_ID, 'run_attempt': SELF_TEST_RUN_ATTEMPT, 'target': ASSET,
    }
    runtime_identity_zip = zip_bytes({'upload-identity.json': json.dumps(runtime_identity).encode()})
    vsix_bytes = b'fixture exact VSIX bytes'
    vsix_manifest = {
        'source_sha': SELF_TEST_SHA, 'original_ref': SELF_TEST_TAG_REF, 'event': 'push',
        'run_id': SELF_TEST_RUN_ID, 'run_attempt': SELF_TEST_RUN_ATTEMPT,
        'version': SELF_TEST_VERSION, 'pnpm_version': '10.13.1',
        'vsix_sha256': hashlib.sha256(vsix_bytes).hexdigest(),
        'lock_hashes': dict(source_lock_hashes),
    }
    vsix_manifest_bytes = json.dumps(vsix_manifest).encode()
    vsix_name = f'magit-lsp-{SELF_TEST_VERSION}.vsix'
    runtime_vsix_zip = zip_bytes({vsix_name: vsix_bytes,
                                  'vsix-manifest.json': vsix_manifest_bytes})
    runtime_vsix_identity = {
        'artifact_id': '102',
        'artifact_digest': 'sha256:' + hashlib.sha256(runtime_vsix_zip).hexdigest(),
        'run_id': SELF_TEST_RUN_ID, 'run_attempt': SELF_TEST_RUN_ATTEMPT,
    }
    runtime_vsix_identity_zip = zip_bytes(
        {'vsix-upload-identity.json': json.dumps(runtime_vsix_identity).encode()})
    runtime_coherent_artifacts = [
        artifact_document(ARTIFACT_NAME, 100,
                         digest='sha256:' + hashlib.sha256(runtime_candidate_zip).hexdigest()),
        artifact_document(f'{ARTIFACT_NAME}-identity', 101,
                         digest='sha256:' + hashlib.sha256(runtime_identity_zip).hexdigest()),
        artifact_document(VSIX_ARTIFACT, 102,
                         digest='sha256:' + hashlib.sha256(runtime_vsix_zip).hexdigest()),
        artifact_document(VSIX_IDENTITY_ARTIFACT, 103,
                         digest='sha256:' + hashlib.sha256(runtime_vsix_identity_zip).hexdigest()),
    ]
    # A Windows-only failure is never part of the required graph, so it must not
    # affect the runtime readiness binding.
    runtime_jobs = coherent_jobs + [
        job_document('native-windows-experimental', 7, conclusion='failure')]
    runtime_event = {'action': 'completed', 'workflow_run': {
        'id': int(SELF_TEST_RUN_ID), 'run_attempt': int(SELF_TEST_RUN_ATTEMPT),
        'workflow_id': SELF_TEST_WORKFLOW_ID, 'event': 'push',
        'head_branch': f'v{SELF_TEST_VERSION}',
        'repository': {'full_name': SELF_TEST_REPOSITORY},
        'head_sha': SELF_TEST_OTHER_SHA}}
    runtime_total_jobs = len(runtime_jobs)
    runtime_total_artifacts = len(runtime_coherent_artifacts)
    runtime_responses = {
        '': {'default_branch': 'main'},
        '/branches/main': {'name': 'main', 'protected': True},
        '/git/ref/heads/main': {'object': {'sha': SELF_TEST_SHA}},
        f'/actions/runs/{SELF_TEST_RUN_ID}': run_binding_fixture(),
        f'/actions/runs/{SELF_TEST_RUN_ID}/jobs?per_page=100&page=1':
            page_document('jobs', runtime_jobs[:3], total=runtime_total_jobs),
        f'/actions/runs/{SELF_TEST_RUN_ID}/jobs?per_page=100&page=2':
            page_document('jobs', runtime_jobs[3:], total=runtime_total_jobs),
        f'/actions/runs/{SELF_TEST_RUN_ID}/artifacts?per_page=100&page=1':
            page_document('artifacts', runtime_coherent_artifacts[:2],
                          total=runtime_total_artifacts),
        f'/actions/runs/{SELF_TEST_RUN_ID}/artifacts?per_page=100&page=2':
            page_document('artifacts', runtime_coherent_artifacts[2:],
                          total=runtime_total_artifacts),
        f'/git/ref/tags/v{SELF_TEST_VERSION}': json.loads(tag_ref_body()),
        **{f'/contents/{name}?ref={SELF_TEST_SHA}': {
            'type': 'file', 'path': name, 'encoding': 'base64',
            'size': len(source_lock_bytes[name]),
            'content': base64.b64encode(source_lock_bytes[name]).decode('ascii')}
           for name in SOURCE_LOCK_FILES},
    }
    runtime_calls = []
    runtime_overrides = {}
    def runtime_fetch(repository, path, token):
        runtime_calls.append((repository, path))
        if path in runtime_overrides:
            return runtime_overrides[path]
        if path not in runtime_responses:
            return 404, '{}'
        return 200, json.dumps(runtime_responses[path])
    saved_env = {key: os.environ.get(key) for key in (
        'GITHUB_REPOSITORY', 'GITHUB_REF', 'GITHUB_SHA',
        'GITHUB_EVENT_NAME', 'GH_TOKEN', 'PUBLICATION_HANDOFF_DIR')}
    try:
        os.environ.update({'GITHUB_REPOSITORY': SELF_TEST_REPOSITORY,
                           'GITHUB_REF': 'refs/heads/main',
                           'GITHUB_SHA': SELF_TEST_SHA,
                           'GITHUB_EVENT_NAME': 'workflow_run', 'GH_TOKEN': 'mock'})
        with tempfile.TemporaryDirectory(prefix='publication-runtime-') as temporary:
            event_path = Path(temporary) / 'event.json'
            event_path.write_text(json.dumps(runtime_event), encoding='utf-8')
            handoff_dir = Path(temporary) / 'handoff'
            os.environ['PUBLICATION_HANDOFF_DIR'] = str(handoff_dir)
            runtime_zip_bytes = {'100': runtime_candidate_zip,
                                 '101': runtime_identity_zip,
                                 '102': runtime_vsix_zip,
                                 '103': runtime_vsix_identity_zip}
            runtime_metadata = {str(item['id']): item
                                for item in runtime_coherent_artifacts}
            transport_calls = []

            def response_for(content):
                class Response:
                    def __enter__(self): return self
                    def __exit__(self, *_): return None
                    def read(self): return content
                return Response()

            def runtime_transport(url, token):
                transport_calls.append(url)
                if url.endswith('/artifacts?per_page=100'):
                    return response_for(json.dumps({
                        'total_count': len(runtime_coherent_artifacts),
                        'artifacts': runtime_coherent_artifacts}).encode())
                if '/actions/artifacts/' in url:
                    remainder = url.split('/actions/artifacts/')[1].split('/')
                    if len(remainder) > 1 and remainder[1] == 'zip':
                        return response_for(runtime_zip_bytes[remainder[0]])
                    return response_for(
                        json.dumps(runtime_metadata[remainder[0]]).encode())
                if '/actions/runs/' in url:
                    return response_for(json.dumps(run_binding_fixture()).encode())
                raise AssertionError(f'unexpected transport URL {url}')
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                runtime_calls.clear()
                handoff = prepare_publication_runtime(str(event_path))
                if (SELF_TEST_REPOSITORY, '/branches/main') not in runtime_calls:
                    fail('self-test: runtime collector must read the protected branch metadata')
                if any('/protection' in path for _, path in runtime_calls):
                    fail('self-test: runtime collector requested the Administration-only '
                         'branch protection endpoint')
            expected_vsix_sha = hashlib.sha256(vsix_bytes).hexdigest()
            if handoff['source_sha'] != SELF_TEST_SHA or \
                    handoff['version'] != SELF_TEST_VERSION or \
                    handoff['original_ref'] != SELF_TEST_TAG_REF or \
                    handoff['vsix']['vsix_sha256'] != expected_vsix_sha or \
                    handoff['native']['artifact_id'] != '100':
                fail('self-test: the runtime handoff must bind source, version, tag '
                     'and the exact VSIX bytes')
            handoff_path = handoff_dir / 'publication-handoff.json'
            if json.loads(handoff_path.read_text(encoding='utf-8')) != handoff:
                fail('self-test: the runtime must write the validated handoff')
            native_archive = f'magit-lsp-v{SELF_TEST_VERSION}-{ASSET}.tar.gz'
            for name in (native_archive, native_archive + '.sha256',
                         'release-manifest.json', 'release-manifest.json.sha256',
                         f'magit-lsp-{SELF_TEST_VERSION}.vsix', 'vsix-manifest.json'):
                persisted = handoff_dir / name
                if not persisted.is_file() or persisted.is_symlink():
                    fail(f'self-test: the runtime must persist {name!r} as a '
                         'regular file')
            if handoff['native']['archive'] != native_archive or \
                    handoff['native']['checksum'] != native_archive + '.sha256' or \
                    Path(handoff['native']['archive']).is_absolute() or \
                    handoff['vsix']['vsix'] != f'magit-lsp-{SELF_TEST_VERSION}.vsix' or \
                    Path(handoff['vsix']['vsix']).is_absolute() or \
                    Path(handoff['vsix']['manifest']).is_absolute():
                fail('self-test: the handoff must record relocatable native/VSIX names')
            # Every lock expectation is fetched at the peeled source SHA; the
            # default branch and the publisher checkout are never consulted.
            expected_lock_paths = [f'/contents/{name}?ref={SELF_TEST_SHA}'
                                   for name in SOURCE_LOCK_FILES]
            missing_lock_paths = [path for path in expected_lock_paths
                                  if (SELF_TEST_REPOSITORY, path) not in runtime_calls]
            if missing_lock_paths:
                fail('self-test: the runtime must fetch every source lock at the '
                     f'bound SHA; missing {missing_lock_paths!r}')
            lock_calls = [path for _, path in runtime_calls if '/contents/' in path]
            if any(f'ref={SELF_TEST_SHA}' not in path for path in lock_calls):
                fail('self-test: every lock fetch must address the bound source '
                     f'SHA, saw {lock_calls!r}')
            for key in ('vsix', 'manifest'):
                if not (handoff_dir / handoff['vsix'][key]).is_file():
                    fail(f'self-test: the handoff {key} path must stay readable')
            # The uploaded directory is relocated by the artifact download; the
            # publisher must resolve every accepted byte from that new location.
            relocated = Path(temporary) / 'relocated-handoff'
            shutil.copytree(handoff_dir, relocated)
            relocated_handoff = relocated / 'publication-handoff.json'
            backend = publisher_backend(publish_release)
            completed = publish_release.publish(
                relocated / 'publication-handoff.json', backend=backend,
                environment=publisher_environment(), event=publisher_event())
            if completed != ['github-release', 'marketplace', 'openvsx']:
                fail(f'self-test: unexpected publication order {completed!r}')
            if [call[0] for call in backend.calls] != [
                    'release_assets', 'create_release', 'release_assets',
                    'marketplace', 'openvsx']:
                fail(f'self-test: publication must proceed GitHub -> Marketplace '
                     f'-> Open VSX, saw {backend.calls!r}')
            lane_paths = {call[1] for call in backend.calls
                          if call[0] in ('marketplace', 'openvsx')}
            if len(lane_paths) != 1 or \
                    Path(lane_paths.pop()).read_bytes() != vsix_bytes:
                fail('self-test: both registry lanes must publish the identical '
                     'accepted VSIX bytes')
            # Exact locked local invocations, never a packaging/build fallback.
            marketplace_argv, openvsx_argv = publish_release.registry_commands(
                'accepted.vsix')
            if marketplace_argv != ('clients/vscode/node_modules/.bin/vsce',
                                    'publish', '--oidc', '--packagePath',
                                    'accepted.vsix') or \
                    openvsx_argv != ('clients/vscode/node_modules/.bin/ovsx',
                                     'publish', '--trusted-publishing',
                                     'accepted.vsix'):
                fail(f'self-test: unexpected registry argv '
                     f'{marketplace_argv!r}/{openvsx_argv!r}')
            for argv in (marketplace_argv, openvsx_argv):
                if argv[1] != 'publish' or any(
                        token in argv for token in ('package', 'build', 'cargo',
                                                    'pnpm', 'npm', 'npx')):
                    fail('self-test: registry commands must publish, not '
                         'build/repackage')
            if publish_release.BUILD_WORKFLOW_NAME != BUILD_WORKFLOW_NAME or \
                    publish_release.BUILD_WORKFLOW_PATH != BUILD_WORKFLOW_PATH or \
                    publish_release.TARGET != ASSET or \
                    publish_release.MANIFEST_FIELDS != MANIFEST_FIELDS:
                fail('self-test: publisher contract constants drifted')
            # Changed bytes, escaped paths and symlinks never reach a destination.
            def relocated_case(label):
                case = Path(temporary) / f'case-{label}'
                shutil.copytree(handoff_dir, case)
                return case / 'publication-handoff.json'

            case = relocated_case('changed-bytes')
            (case.parent / native_archive).write_bytes(b'changed archive bytes')
            backend = publisher_backend(publish_release)
            expect_publish_error(
                publish_release, 'publisher with changed archive bytes',
                publish_release.publish, case, backend=backend,
                environment=publisher_environment(), event=publisher_event(),
                expected='archive bytes differ from the handoff')
            if backend.calls:
                fail('self-test: a tampered input reached a destination')
            for escaped, expected in (('../escape.tar.gz',
                                       'escapes the handoff directory'),
                                      ('/etc/passwd', 'escapes the handoff directory'),
                                      ('nested/archive.tar.gz',
                                       'escapes the handoff directory'),
                                      ('..\\escape.tar.gz', 'invalid')):
                expect_publish_error(
                    publish_release, f'publisher with escaped path {escaped!r}',
                    publish_release.asset_path, handoff_dir, escaped, 'archive',
                    expected=expected)
            case = relocated_case('symlink')
            outside_vsix = Path(temporary) / 'outside.vsix'
            shutil.copy(case.parent / handoff['vsix']['vsix'], outside_vsix)
            (case.parent / handoff['vsix']['vsix']).unlink()
            (case.parent / handoff['vsix']['vsix']).symlink_to(outside_vsix)
            expect_publish_error(
                publish_release, 'publisher with a symlinked VSIX',
                publish_release.publish, case,
                backend=publisher_backend(publish_release),
                environment=publisher_environment(), event=publisher_event(),
                expected='symbolic link')
            # Mismatched source, tag, version and run are refused before mutation.
            for label, mutation, expected in (
                    ('source', ('head_sha', SELF_TEST_OTHER_SHA),
                     'source differs from the trusted tag commit'),
                    ('tag', ('head_branch', 'v9.9.9'), 'event tag'),
                    ('run', ('id', int(SELF_TEST_RUN_ID) + 1),
                     'run identity differs')):
                event = publisher_event()
                event['workflow_run'][mutation[0]] = mutation[1]
                expect_publish_error(
                    publish_release, f'publisher with a mismatched {label}',
                    publish_release.publish,
                    relocated_case(f'mismatch-{label}'),
                    backend=publisher_backend(publish_release),
                    environment=publisher_environment(), event=event,
                    expected=expected)
            case = relocated_case('version')
            document = json.loads(case.read_text(encoding='utf-8'))
            document['version'] = '9.9.9'
            case.write_text(json.dumps(document), encoding='utf-8')
            expect_publish_error(
                publish_release, 'publisher with a mismatched version',
                publish_release.publish, case,
                backend=publisher_backend(publish_release),
                environment=publisher_environment(), event=publisher_event(),
                expected='is not the version tag')
            # Missing OIDC and every PAT fallback fail closed.
            missing_oidc = publisher_environment()
            missing_oidc.pop('ACTIONS_ID_TOKEN_REQUEST_URL')
            missing_oidc.pop('ACTIONS_ID_TOKEN_REQUEST_TOKEN')
            expect_publish_error(
                publish_release, 'publisher without OIDC',
                publish_release.publish, relocated_handoff,
                backend=publisher_backend(publish_release),
                environment=missing_oidc, event=publisher_event(),
                expected='OIDC environment')
            for name in ('VSCE_PAT', 'OVSX_PAT'):
                pat_only = dict(missing_oidc, **{name: 'fixture-pat'})
                expect_publish_error(
                    publish_release, f'publisher with only {name}',
                    publish_release.publish, relocated_handoff,
                    backend=publisher_backend(publish_release),
                    environment=pat_only, event=publisher_event(),
                    expected='PAT fallback')
            # A GitHub release that fails during asset upload or its
            # post-creation readback may already exist. That must be reported as
            # partial/unverified GitHub progress requiring inspection, never as
            # a clean failure, and it still blocks both registry lanes.
            for label, failure in (('asset upload', 'upload'),
                                   ('post-creation readback', 'readback')):
                backend = publisher_backend(publish_release, fail=failure)
                expect_publish_error(
                    publish_release,
                    f'publisher whose GitHub {label} fails after creation',
                    publish_release.publish, relocated_handoff, backend=backend,
                    environment=publisher_environment(), event=publisher_event(),
                    expected='partial or unverified',
                    required=('require manual inspection',
                              'neither registry was attempted',
                              'completed none',
                              'pending github-release, marketplace, openvsx'),
                    forbidden=('no GitHub mutation was attempted',))
                if not any(call[0] == 'create_release' for call in backend.calls):
                    fail(f'self-test: a GitHub {label} failure must follow a '
                         'recorded release creation')
                if any(call[0] in ('marketplace', 'openvsx') for call in backend.calls):
                    fail(f'self-test: a GitHub {label} failure must block both '
                         'registries')
            # A lookup failure happens before creation, so it stays an honest
            # no-mutation diagnostic rather than claiming unknown side effects.
            backend = publisher_backend(publish_release, fail='lookup')
            expect_publish_error(
                publish_release, 'publisher whose GitHub lookup fails',
                publish_release.publish, relocated_handoff, backend=backend,
                environment=publisher_environment(), event=publisher_event(),
                expected='no GitHub mutation was attempted',
                required=('completed none', 'pending github-release, marketplace, '
                          'openvsx'),
                forbidden=('partial or unverified',))
            if [call[0] for call in backend.calls] != ['release_assets']:
                fail('self-test: a GitHub lookup failure must happen before any '
                     'mutation or registry call')
            # The command exit status makes the uncertainty visible to the
            # release workflow: a real upload failure after creation exits 1 and
            # prints the inspection warning.
            publisher_event_path = Path(temporary) / 'publisher-event.json'
            publisher_event_path.write_text(json.dumps(publisher_event()),
                                            encoding='utf-8')
            mutation_lookup = ['gh', 'api',
                               f'repos/{SELF_TEST_REPOSITORY}/releases/tags/'
                               f'v{SELF_TEST_VERSION}']
            calls, run = publisher_command(
                [(1, '', 'gh: Not Found (HTTP 404)'),
                 (1, '', 'gh: release asset upload failed')])
            diagnostic = io.StringIO()
            with patch.object(subprocess, 'run', run), \
                    patch.dict(os.environ, publisher_environment(
                        GITHUB_EVENT_PATH=str(publisher_event_path)), clear=True), \
                    patch.object(sys, 'stderr', diagnostic):
                status = publish_release.main(
                    ['--handoff', str(relocated_handoff)])
            if status != 1 or 'partial or unverified' not in diagnostic.getvalue():
                fail('self-test: an uncertain GitHub release mutation must exit '
                     f'nonzero with an inspection diagnostic; saw {status!r} and '
                     f'{diagnostic.getvalue()!r}')
            if calls[0] != mutation_lookup or \
                    calls[1][:3] != ['gh', 'release', 'create'] \
                    or registry_commands_run(calls):
                fail(f'self-test: a real upload failure must attempt creation '
                     f'only; saw {calls!r}')
            # An existing release with different bytes is a conflict, never an
            # overwrite and never a registry publication.
            conflict = {name: 'sha256:' + '0' * 64
                        for name in (native_archive, native_archive + '.sha256',
                                     'release-manifest.json',
                                     'release-manifest.json.sha256',
                                     f'magit-lsp-{SELF_TEST_VERSION}.vsix')}
            backend = publisher_backend(publish_release, release=conflict)
            expect_publish_error(
                publish_release, 'publisher with a conflicting GitHub release',
                publish_release.publish, relocated_handoff, backend=backend,
                environment=publisher_environment(), event=publisher_event(),
                expected='conflicting assets; refusing to overwrite')
            if [call[0] for call in backend.calls] != ['release_assets']:
                fail('self-test: a conflicting release must not be mutated or '
                     'published')
            # The real backend parser reads the GitHub response itself. A draft,
            # a wrong tag or conflicting bytes fails closed before either
            # registry, for both an existing release and a creation readback.
            tag = f'v{SELF_TEST_VERSION}'
            release_names = (native_archive, native_archive + '.sha256',
                             'release-manifest.json',
                             'release-manifest.json.sha256',
                             f'magit-lsp-{SELF_TEST_VERSION}.vsix')
            exact_assets = {name: 'sha256:' + hashlib.sha256(
                (relocated / name).read_bytes()).hexdigest()
                for name in release_names}
            lookup = ['gh', 'api',
                      f'repos/{SELF_TEST_REPOSITORY}/releases/tags/{tag}']
            for label, body, expected in (
                    ('draft', publisher_release_body(tag, exact_assets, draft=True),
                     'is not published'),
                    ('wrong-tag', publisher_release_body('v9.9.9', exact_assets),
                     'returned tag')):
                calls, run = publisher_command([(0, body, '')])
                with patch.object(subprocess, 'run', run):
                    expect_publish_error(
                        publish_release,
                        f'publisher with an existing {label} release',
                        publish_release.publish, relocated_handoff,
                        backend=publish_release.CommandBackend(root=str(relocated)),
                        environment=publisher_environment(),
                        event=publisher_event(), expected=expected)
                if calls != [lookup]:
                    fail(f'self-test: an existing {label} release must fail '
                         f'before any registry; saw {calls!r}')
            conflict_body = publisher_release_body(
                tag, {name: 'sha256:' + '0' * 64 for name in release_names})
            calls, run = publisher_command([(0, conflict_body, '')])
            with patch.object(subprocess, 'run', run):
                expect_publish_error(
                    publish_release,
                    'publisher with a real conflicting GitHub release',
                    publish_release.publish, relocated_handoff,
                    backend=publish_release.CommandBackend(root=str(relocated)),
                    environment=publisher_environment(),
                    event=publisher_event(),
                    expected='conflicting assets; refusing to overwrite')
            if calls != [lookup]:
                fail(f'self-test: a real conflicting release must fail before '
                     f'any registry; saw {calls!r}')
            # Creation succeeds, but the readback must still prove published
            # identity before the registry lanes run.
            for label, body, expected in (
                    ('draft', publisher_release_body(tag, exact_assets, draft=True),
                     'is not published'),
                    ('wrong-tag', publisher_release_body('v9.9.9', exact_assets),
                     'returned tag')):
                calls, run = publisher_command(
                    [(1, '', 'gh: Not Found (HTTP 404)'), (0, '', ''),
                     (0, body, '')])
                with patch.object(subprocess, 'run', run):
                    expect_publish_error(
                        publish_release,
                        f'publisher whose created release reads back {label}',
                        publish_release.publish, relocated_handoff,
                        backend=publish_release.CommandBackend(root=str(relocated)),
                        environment=publisher_environment(),
                        event=publisher_event(), expected=expected,
                        required=('partial or unverified',
                                  'neither registry was attempted'))
                if calls[0] != lookup or calls[1][:3] != ['gh', 'release', 'create'] \
                        or registry_commands_run(calls):
                    fail(f'self-test: a created {label} readback must fail before '
                         f'any registry; saw {calls!r}')
            # A readback that cannot be read at all is equally uncertain.
            calls, run = publisher_command(
                [(1, '', 'gh: Not Found (HTTP 404)'), (0, '', ''),
                 (1, '', 'gh: HTTP 500: server error')])
            with patch.object(subprocess, 'run', run):
                expect_publish_error(
                    publish_release,
                    'publisher whose created release readback errors',
                    publish_release.publish, relocated_handoff,
                    backend=publish_release.CommandBackend(root=str(relocated)),
                    environment=publisher_environment(), event=publisher_event(),
                    expected='GitHub release lookup for',
                    required=('partial or unverified',
                              'neither registry was attempted'))
            if calls[0] != lookup or calls[1][:3] != ['gh', 'release', 'create'] \
                    or registry_commands_run(calls):
                fail(f'self-test: an errored readback must attempt creation only; '
                     f'saw {calls!r}')
            # A lookup that fails before creation is not a partial mutation.
            calls, run = publisher_command(
                [(1, '', 'gh: HTTP 500: server error')])
            with patch.object(subprocess, 'run', run):
                expect_publish_error(
                    publish_release, 'publisher whose release lookup errors',
                    publish_release.publish, relocated_handoff,
                    backend=publish_release.CommandBackend(root=str(relocated)),
                    environment=publisher_environment(), event=publisher_event(),
                    expected='no GitHub mutation was attempted',
                    forbidden=('partial or unverified',))
            if calls != [lookup]:
                fail(f'self-test: a failed release lookup must not create or '
                     f'publish; saw {calls!r}')
            # An exact published release is still accepted and both registry
            # lanes run against the same VSIX.
            calls, run = publisher_command(
                [(0, publisher_release_body(tag, exact_assets), ''),
                 (0, '', ''), (0, '', '')])
            with patch.object(subprocess, 'run', run):
                completed = publish_release.publish(
                    relocated_handoff,
                    backend=publish_release.CommandBackend(root=str(relocated)),
                    environment=publisher_environment(), event=publisher_event())
            if completed != ['github-release', 'marketplace', 'openvsx']:
                fail(f'self-test: an exact published release must publish in '
                     f'order, saw {completed!r}')
            registries = registry_commands_run(calls)
            if calls[0] != lookup or len(registries) != 2 or \
                    not registries[0][0].endswith('vsce') or \
                    not registries[1][0].endswith('ovsx') or \
                    registries[0][1:] != ['publish', '--oidc', '--packagePath',
                                          str((relocated / handoff['vsix']['vsix']).resolve())]:
                fail(f'self-test: an exact published release must reach both '
                     f'registry lanes once; saw {calls!r}')
            # Partial registry completion is reported, never atomic success.
            backend = publisher_backend(publish_release, fail='marketplace')
            expect_publish_error(
                publish_release, 'publisher with a marketplace failure',
                publish_release.publish, relocated_handoff, backend=backend,
                environment=publisher_environment(), event=publisher_event(),
                expected='completed github-release; pending marketplace, openvsx')
            if any(call[0] == 'openvsx' for call in backend.calls):
                fail('self-test: a marketplace failure must leave Open VSX pending')
            backend = publisher_backend(publish_release, fail='openvsx')
            expect_publish_error(
                publish_release, 'publisher with an Open VSX failure',
                publish_release.publish, relocated_handoff, backend=backend,
                environment=publisher_environment(), event=publisher_event(),
                expected='completed github-release, marketplace; pending openvsx')
            required_paths = [f'/actions/runs/{SELF_TEST_RUN_ID}',
                          f'/actions/runs/{SELF_TEST_RUN_ID}/jobs?per_page=100&page=2',
                          f'/actions/runs/{SELF_TEST_RUN_ID}/artifacts?per_page=100&page=2',
                          f'/git/ref/tags/v{SELF_TEST_VERSION}']
            missing_runtime_paths = [path for path in required_paths
                                 if (SELF_TEST_REPOSITORY, path) not in runtime_calls]
            if missing_runtime_paths:
                fail(f'self-test: runtime collector missed API paths {missing_runtime_paths!r}; calls={runtime_calls!r}')
            # Branch protection is read through ordinary branch metadata. Only a
            # positive boolean counts; forbidden, missing, unprotected and
            # malformed responses all fail closed.
            for label, override, expected in (
                    ('forbidden',
                     (403, '{"message":"Resource not accessible by integration"}'),
                     'branch metadata returned HTTP 403'),
                    ('missing', (404, '{}'),
                     'branch metadata returned HTTP 404'),
                    ('unprotected',
                     (200, json.dumps({'name': 'main', 'protected': False})),
                     'does not report a protected branch'),
                    ('missing status', (200, json.dumps({'name': 'main'})),
                     'does not report a protected branch'),
                    ('non-boolean status',
                     (200, json.dumps({'name': 'main', 'protected': 'true'})),
                     'does not report a protected branch'),
                    ('malformed', (200, '[]'), 'metadata: expected object')):
                runtime_overrides['/branches/main'] = override
                with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                        patch.object(runtime_transport_module, 'request', runtime_transport):
                    expect_check(f'runtime collector rejects {label} branch metadata',
                                 prepare_publication_runtime, str(event_path),
                                 expected=expected)
            runtime_overrides.clear()
            # The VSIX is fetched through the same verifier the installed
            # consumers use: exact run/attempt/listing and byte digest checks.
            for fragment in (f'/actions/runs/{SELF_TEST_RUN_ID}/artifacts?per_page=100',
                             '/actions/artifacts/102/zip', '/actions/artifacts/102'):
                if not any(fragment in url for url in transport_calls):
                    fail(f'self-test: runtime VSIX verification missed {fragment}')
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}'] = run_binding_fixture(name='untrusted')
            runtime_calls.clear()
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects mismatched workflow',
                             prepare_publication_runtime, str(event_path),
                             expected='workflow name')
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}'] = run_binding_fixture()
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}/artifacts?per_page=100&page=1'] = \
                page_document('artifacts', [
                    artifact_document(ARTIFACT_NAME, 100,
                                      digest='sha256:' + '0' * 64),
                    runtime_coherent_artifacts[1]], total=runtime_total_artifacts)
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects a changed API artifact digest',
                             prepare_publication_runtime, str(event_path),
                             expected='artifact digest does not match')
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}/artifacts?per_page=100&page=1'] = \
                page_document('artifacts', runtime_coherent_artifacts[:2],
                              total=runtime_total_artifacts)
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}/jobs?per_page=100&page=2'] = \
                page_document('jobs', [], total=runtime_total_jobs)
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects a missing listing page',
                             prepare_publication_runtime, str(event_path),
                             expected='made no pagination progress')
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}/jobs?per_page=100&page=2'] = \
                page_document('jobs', runtime_jobs[3:], total=runtime_total_jobs)
            # A skipped required aggregate blocks the handoff even when every
            # other required job succeeded.
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}/jobs?per_page=100&page=2'] = \
                page_document('jobs', [
                    job if job['name'] != RELEASE_READINESS_JOB
                    else job_document(RELEASE_READINESS_JOB, 5,
                                      conclusion='skipped')
                    for job in runtime_jobs[3:]], total=runtime_total_jobs)
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects a skipped required aggregate',
                             prepare_publication_runtime, str(event_path),
                             expected='not success')
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}/jobs?per_page=100&page=2'] = \
                page_document('jobs', runtime_jobs[3:], total=runtime_total_jobs)
            runtime_responses['/git/ref/heads/main'] = {'object': {'sha': SELF_TEST_OTHER_SHA}}
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects the wrong default checkout',
                             prepare_publication_runtime, str(event_path),
                             expected='checkout SHA is not the current default branch SHA')
            runtime_responses['/git/ref/heads/main'] = {'object': {'sha': SELF_TEST_SHA}}
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}'] = \
                run_binding_fixture(run_attempt=int(SELF_TEST_RUN_ATTEMPT) + 1)
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects a changed run attempt',
                             prepare_publication_runtime, str(event_path),
                             expected='binding attempt')
            runtime_responses[f'/actions/runs/{SELF_TEST_RUN_ID}'] = run_binding_fixture()
            runtime_responses[f'/git/ref/tags/v{SELF_TEST_VERSION}'] = \
                json.loads(tag_ref_body(sha=SELF_TEST_OTHER_SHA))
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects a moved original tag',
                             prepare_publication_runtime, str(event_path),
                             expected='not the manifest source')
            runtime_responses[f'/git/ref/tags/v{SELF_TEST_VERSION}'] = json.loads(tag_ref_body())
            runtime_calls.clear()
            original_runtime_transport = runtime_transport
            def changed_runtime_transport(url, token):
                content = original_runtime_transport(url, token).read()
                if url.endswith('/zip'):
                    content += b'changed'
                return response_for(content)
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', changed_runtime_transport):
                expect_check('runtime collector rejects changed handoff bytes',
                             prepare_publication_runtime, str(event_path),
                             expected='transport digest mismatch')
            # The VSIX manifest still names the producer's hash while the
            # uploaded bytes changed, so the exact-byte verifier refuses it even
            # though the artifact transport digest is internally consistent.
            tampered_vsix_zip = zip_bytes({vsix_name: b'changed VSIX bytes',
                                           'vsix-manifest.json': vsix_manifest_bytes})
            tampered_identity = dict(
                runtime_vsix_identity,
                artifact_digest='sha256:' + hashlib.sha256(tampered_vsix_zip).hexdigest())
            tampered_identity_zip = zip_bytes(
                {'vsix-upload-identity.json': json.dumps(tampered_identity).encode()})
            runtime_zip_bytes['102'] = tampered_vsix_zip
            runtime_zip_bytes['103'] = tampered_identity_zip
            runtime_coherent_artifacts[2]['digest'] = tampered_identity['artifact_digest']
            runtime_coherent_artifacts[3]['digest'] = (
                'sha256:' + hashlib.sha256(tampered_identity_zip).hexdigest())
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects changed VSIX bytes',
                             prepare_publication_runtime, str(event_path),
                             expected='VSIX digest differs from manifest')
            # A VSIX identity naming another artifact is refused even when the
            # downloaded bytes hash to the recorded artifact digest.
            mismatched_identity = dict(runtime_vsix_identity, artifact_id='999')
            mismatched_identity_zip = zip_bytes(
                {'vsix-upload-identity.json': json.dumps(mismatched_identity).encode()})
            runtime_zip_bytes['102'] = runtime_vsix_zip
            runtime_zip_bytes['103'] = mismatched_identity_zip
            runtime_coherent_artifacts[2]['digest'] = runtime_vsix_identity['artifact_digest']
            runtime_coherent_artifacts[3]['digest'] = (
                'sha256:' + hashlib.sha256(mismatched_identity_zip).hexdigest())
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects a mismatched VSIX identity',
                             prepare_publication_runtime, str(event_path),
                             expected='upload identity does not bind candidate/run/attempt')
            # Missing or malformed source lock data fails closed before any
            # handoff or publication; it never falls back to the checkout or
            # the default branch.
            handoff_path = handoff_dir / 'publication-handoff.json'
            handoff_path.unlink()
            cargo_lock_path = f'/contents/Cargo.lock?ref={SELF_TEST_SHA}'
            runtime_overrides[cargo_lock_path] = (404, '{}')
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects an unavailable source lock',
                             prepare_publication_runtime, str(event_path),
                             expected=f'release source Cargo.lock at {SELF_TEST_SHA} '
                                      'returned HTTP 404')
            if handoff_path.exists():
                fail('self-test: unavailable source lock data must fail before '
                     'the handoff')
            malformed_source_locks = (
                ('a lock from another path',
                 {'type': 'file', 'path': 'flake.lock', 'encoding': 'base64',
                  'size': 1, 'content': 'eA=='},
                 'is not the tracked file'),
                ('a directory in place of the lock',
                 {'type': 'dir', 'path': 'Cargo.lock', 'encoding': 'base64',
                  'size': 1, 'content': 'eA=='},
                 'is not the tracked file'),
                ('a lock with another encoding',
                 {'type': 'file', 'path': 'Cargo.lock', 'encoding': 'utf-8',
                  'size': 1, 'content': 'eA=='},
                 'is not base64-encoded content'),
                ('an empty lock',
                 {'type': 'file', 'path': 'Cargo.lock', 'encoding': 'base64',
                  'size': 0, 'content': ''},
                 'has no content'),
                ('a lock whose size does not match',
                 {'type': 'file', 'path': 'Cargo.lock', 'encoding': 'base64',
                  'size': 2, 'content': 'eA=='},
                 'size differs from its content'),
                ('invalid base64 lock content',
                 {'type': 'file', 'path': 'Cargo.lock', 'encoding': 'base64',
                  'size': 1, 'content': '!!!!'},
                 'content is not valid base64'),
            )
            for description, document, reason in malformed_source_locks:
                runtime_overrides[cargo_lock_path] = (200, json.dumps(document))
                with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                        patch.object(runtime_transport_module, 'request',
                                     runtime_transport):
                    expect_check(f'runtime collector rejects {description}',
                                 prepare_publication_runtime, str(event_path),
                                 expected=f'release source Cargo.lock {reason}')
                if handoff_path.exists():
                    fail(f'self-test: {description} must fail before the handoff')
            runtime_overrides.clear()
            # A native manifest whose locks name another revision is rejected
            # against the bound source even though its bytes are self-coherent.
            mismatched_native_manifest = dict(
                runtime_manifest,
                lock_hashes={**runtime_manifest['lock_hashes'],
                             'Cargo.lock': '0' * 64})
            mismatched_native_bytes = json.dumps(mismatched_native_manifest).encode()
            mismatched_native_assets = dict(
                runtime_assets, **{
                    'release-manifest.json': mismatched_native_bytes,
                    'release-manifest.json.sha256':
                        f'{hashlib.sha256(mismatched_native_bytes).hexdigest()}  '
                        'release-manifest.json\n'.encode()})
            mismatched_native_zip = zip_bytes(mismatched_native_assets)
            mismatched_native_identity = dict(
                runtime_identity,
                artifact_digest='sha256:' +
                hashlib.sha256(mismatched_native_zip).hexdigest())
            mismatched_native_identity_zip = zip_bytes(
                {'upload-identity.json': json.dumps(
                    mismatched_native_identity).encode()})
            runtime_zip_bytes['100'] = mismatched_native_zip
            runtime_zip_bytes['101'] = mismatched_native_identity_zip
            runtime_coherent_artifacts[0]['digest'] = \
                mismatched_native_identity['artifact_digest']
            runtime_coherent_artifacts[1]['digest'] = (
                'sha256:' + hashlib.sha256(
                    mismatched_native_identity_zip).hexdigest())
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects manifest locks from another '
                             'source', prepare_publication_runtime, str(event_path),
                             expected='lock_hashes differ from the bound release source')
            runtime_zip_bytes['100'] = runtime_candidate_zip
            runtime_zip_bytes['101'] = runtime_identity_zip
            runtime_coherent_artifacts[0]['digest'] = runtime_identity['artifact_digest']
            runtime_coherent_artifacts[1]['digest'] = (
                'sha256:' + hashlib.sha256(runtime_identity_zip).hexdigest())
            # The same holds for the VSIX manifest's source lock map.
            mismatched_vsix_manifest = dict(
                vsix_manifest,
                lock_hashes={**vsix_manifest['lock_hashes'],
                             'clients/vscode/pnpm-lock.yaml': '0' * 64})
            mismatched_vsix_zip = zip_bytes({
                vsix_name: vsix_bytes,
                'vsix-manifest.json': json.dumps(
                    mismatched_vsix_manifest).encode()})
            mismatched_vsix_identity = dict(
                runtime_vsix_identity,
                artifact_digest='sha256:' +
                hashlib.sha256(mismatched_vsix_zip).hexdigest())
            mismatched_vsix_identity_zip = zip_bytes(
                {'vsix-upload-identity.json': json.dumps(
                    mismatched_vsix_identity).encode()})
            runtime_zip_bytes['102'] = mismatched_vsix_zip
            runtime_zip_bytes['103'] = mismatched_vsix_identity_zip
            runtime_coherent_artifacts[2]['digest'] = \
                mismatched_vsix_identity['artifact_digest']
            runtime_coherent_artifacts[3]['digest'] = (
                'sha256:' + hashlib.sha256(
                    mismatched_vsix_identity_zip).hexdigest())
            with patch.object(sys.modules[__name__], 'fetch_api', runtime_fetch), \
                    patch.object(runtime_transport_module, 'request', runtime_transport):
                expect_check('runtime collector rejects VSIX locks from another '
                             'source', prepare_publication_runtime, str(event_path),
                             expected='VSIX manifest provenance does not match')
            runtime_zip_bytes['102'] = runtime_vsix_zip
            runtime_zip_bytes['103'] = runtime_vsix_identity_zip
            runtime_coherent_artifacts[2]['digest'] = \
                runtime_vsix_identity['artifact_digest']
            runtime_coherent_artifacts[3]['digest'] = (
                'sha256:' + hashlib.sha256(runtime_vsix_identity_zip).hexdigest())
    finally:
        for key, value in saved_env.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value

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
        with tempfile.TemporaryDirectory(prefix='magit-lsp-vsix-self-test-') as fixture:
            test_vsix_binding_fixtures(fixture)
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
            ('manual trigger bypass', (('  workflow_run:', '  workflow_dispatch:', 1),),
             'trigger must be workflow_run only'),
            ('default SHA as release source', (('github.event.repository.default_branch', 'github.sha', 1),),
             'forbidden legacy/bypass path'),
            ('write validation permission', (('      contents: read', '      contents: write', 1),),
             'validate must be read-only'),
            ('missing protected environment', (('    environment: release-publish\n', '', 1),),
             'must require release-publish environment'),
            ('PAT fallback', (('    steps:', '    env: { VSCE_PAT: ${{ secrets.VSCE_PAT }} }\n    steps:', 1),),
             'forbidden legacy/bypass path'),
            ('publish disabled by a literal false', (('    if: ${{ github.event.workflow_run.event', '    if: ${{ false }} && github.event.workflow_run.event', 1),),
             'publish must gate on a successful push-tag build'),
            ('publish without a validate dependency', (('    needs: validate\n', '', 1),),
             'publish must run only after the validate job'),
            ('publish without the push-origin guard',
             (("github.event.workflow_run.event == 'push'",
               "github.event.workflow_run.event == 'schedule'", 1),),
             "publish guard must require \"github.event.workflow_run.event == 'push'\""),
            ('publish without the success guard',
             (('github.event.workflow_run.conclusion == \'success\'',
               'github.event.workflow_run.conclusion != \'failure\'', 1),),
             'publish guard must require "github.event.workflow_run.conclusion == \'success\'"'),
            ('publish without the same-repository guard',
             ((' && github.event.workflow_run.head_repository.full_name == github.repository', '', 1),),
             'publish guard must require \'github.event.workflow_run.head_repository.full_name == github.repository\''),
            ('publish with an unconditional guard',
             (('    if: ${{ github.event.workflow_run.event', '    if: ${{ true }}\n    # github.event.workflow_run.event', 1),),
             'publish guard must require'),
            ('untrusted producer identity', (('Release artifact build (no publication)', 'Untrusted producer', 1),),
             'missing trusted collector contract'),
            ('untrusted producer checkout', (('ref: ${{ github.event.repository.default_branch }}',
                                                'ref: ${{ github.event.workflow_run.head_sha }}', 1),),
             'validate must check out the protected default branch'),
            ('publisher without the immutable handoff download',
             (('        uses: actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131 # v7.0.0',
               '        uses: actions/upload-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131 # v7.0.0', 1),),
             'missing immutable handoff/publication contract'),
            ('publisher downloading the handoff by mutable name',
             (('          artifact-ids: ${{ needs.validate.outputs.handoff_artifact_id }}',
               '          artifact-ids: ${{ needs.validate.outputs.handoff_artifact_id }}\n'
               '          name: publication-handoff', 1),),
             'must use the immutable artifact id'),
            ('publisher without the uploaded artifact id',
             (('      handoff_artifact_id: ${{ steps.handoff-upload.outputs.artifact-id }}',
               '      handoff_artifact_id: fixture-id', 1),),
             'missing immutable handoff/publication contract'),
            ('publisher rebuilding the VSIX',
             (('      - name: Install the frozen publication tooling\n',
               '      - name: Install the frozen publication tooling\n'
               '      - run: make vscode-package\n', 1),),
             'forbidden legacy/bypass path'),
            ('publisher without the frozen tooling',
             ((f'        run: {DEPENDENCY_INSTALL}\n', '', 1),),
             'missing immutable handoff/publication contract'),
            ('publisher without the declared make interface',
             (('        run: make release-publish HANDOFF=build/release/publication-handoff/publication-handoff.json',
               '        run: true', 1),),
             'missing immutable handoff/publication contract'),
        ):
            expect_check(label, check_workflow,
                         mutated_workflow(directory, label, mutations),
                         expected=expected)

        # The build workflow keeps one pinned pipeline handoff: fixed
        # toolchain, version producer, shared build and shared artifact check.
        for label, mutations, expected in (
             ('build workflow without the VSIX producer',
              (('  vsix:\n', '  vsix-disabled:\n', 1),),
              'missing VSIX producer job'),
             ('VSIX producer using multiple platform builds',
              (('  vsix:\n    needs: source-checks\n    runs-on: ubuntu-24.04\n',
                '  vsix:\n    needs: source-checks\n    runs-on: ubuntu-24.04\n'
                '    strategy:\n      matrix:\n        os: [ubuntu-24.04, macos-26]\n', 1),),
              'platform-independent'),
             ('VSIX producer taking runner-provided Node and pnpm',
              (('      - name: Validate version and package the VSIX\n',
                '      - uses: actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1 # v7.1.0\n'
                '        with:\n          node-version: \'24\'\n'
                '      - name: Validate version and package the VSIX\n', 1),),
              'flake dev shell'),
             ('VSIX producer packaging outside the flake dev shell',
              (("        run: nix develop --no-update-lock-file -c sh -c 'python3 "
                "scripts/check_release_automation.py --validate-release-ref && "
                "make vscode-package'",
                "        run: sh -c 'python3 scripts/check_release_automation.py "
                "--validate-release-ref && make vscode-package'", 1),),
              'flake dev shell'),
             ('VSIX producer rebuilding native binary',
              (('      - name: Validate version and package the VSIX\n',
                '      - run: cargo build\n      - name: Validate version and package the VSIX\n', 1),),
              'must not rebuild the native server'),
             ('VSIX producer without run-attempt binding',
              (('          RELEASE_RUN_ATTEMPT: ${{ github.run_attempt }}\n'
                '          RELEASE_VERSION: ${{ env.RELEASE_VERSION }}',
                '          RELEASE_RUN_ATTEMPT: fixture-attempt\n'
                '          RELEASE_VERSION: ${{ env.RELEASE_VERSION }}', 1),),
              'VSIX producer must bind source/ref/event/run/attempt'),
             ('VSIX producer without provenance generation',
              (('--write-vsix-binding', '--write-other-binding', 1),),
              "missing '--write-vsix-binding'"),
             ('VSIX producer granted publication rights',
              (('  vsix:\n    needs: source-checks\n',
                '  vsix:\n    needs: source-checks\n    permissions:\n'
                '      contents: write\n', 1),),
              'not read-only'),
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
             ('VSIX provenance inferring source from another run',
              (('          RELEASE_SOURCE_SHA: ${{ github.sha }}\n'
                '          RELEASE_EVENT: ${{ github.event_name }}',
                '          RELEASE_SOURCE_SHA: ${{ github.event.workflow_run.head_sha }}\n'
                '          RELEASE_EVENT: ${{ github.event_name }}', 1),),
              'VSIX producer must bind source/ref/event/run/attempt'),
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
             ('build workflow omitting fallback run attempt evidence',
              (('          RELEASE_RUN_ATTEMPT: ${{ github.run_attempt }}\n'
                '          RELEASE_BUILD_TARGET: x86_64-unknown-linux-gnu',
                '          RELEASE_RUN_ATTEMPT: fixture-attempt\n'
                '          RELEASE_BUILD_TARGET: x86_64-unknown-linux-gnu', 1),),
              'github.run_attempt'),
            ('build workflow omitting the upload identity file',
             (('          path: build/release/upload-identity.json',
               '          path: build/release/provenance.json', 1),),
             'upload-identity.json'),
             ('build workflow renaming the fallback candidate the publisher consumes',
              (('          name: linux-x86_64-gnu-release-candidate',
                '          name: release-candidate', 1),),
              'expected \'linux-x86_64-gnu-release-candidate\''),
             ('build workflow selecting an experimental artifact for production',
              (('          name: linux-x86_64-gnu-release-candidate',
                '          name: native-linux-x86_64-gnu-experimental', 1),),
              'complete identity-bound hosted evidence'),
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
            ('build workflow masking a failed native row',
             (('  native-candidates:\n',
               '  native-candidates:\n    continue-on-error: true\n', 1),),
             'continue-on-error would mask a failed Linux/macOS row'),
            ('build workflow naming a native candidate as production',
             (('            artifact: native-macos-arm64-experimental',
               '            artifact: macos-arm64-release-candidate', 1),),
             'experimental artifact'),
            ('build workflow building a native candidate outside its locked shell',
             (('.#release-native', '.#release-naive', None),),
             '.#release-native'),
             ('build workflow native candidate inheriting the dev shell Nix flags',
              (('env -u NIX_CFLAGS_COMPILE -u NIX_LDFLAGS ', '', 1),),
              'clear the dev shell Nix flag'),
             ('build workflow native candidate skipping the shared pipeline',
              (('nix develop --no-update-lock-file .#release-native -c python3 '
                'scripts/check_release_pipeline.py artifacts',
                'nix develop --no-update-lock-file .#release-native -c true', 1),),
              'shared pipeline'),
             ('build workflow missing a native consumer host',
              (('  native-consumers:', '  native-consumers-disabled:', 1),),
              "missing mandatory job 'native-consumers'"),
             ('build workflow native consumers with Nix',
              (('    needs: native-candidates\n',
                '    needs: native-candidates\n    steps:\n      - run: nix develop\n', 1),),
              'without Nix'),
             ('build workflow native consumer with a build command',
              (('      - name: Run native ABI and functional checks\n',
                '      - run: cargo build\n      - name: Run native ABI and functional checks\n', 1),),
              'must not build or install tools'),
             ('build workflow optional native consumer',
              (('  native-consumers:\n', '  native-consumers:\n    continue-on-error: true\n', 1),),
              'must be mandatory'),
             ('build workflow native consumer without immutable run binding',
              (('--attempt "${{ github.run_attempt }}"', '--attempt "1"', 1),),
              'github.run_attempt'),
             ('build workflow native consumer invoking the downloader by script path',
              (('python3 -m scripts.download_native_candidate\n',
                'python3 scripts/download_native_candidate.py\n', 1),),
              'must include'),
             ('build workflow native consumer without functional probe',
              (('scripts/check_native_artifact.py --functional', 'true', 1),),
              'scripts/check_native_artifact.py --functional'),
             ('build workflow native consumer without failure evidence',
              (('native-evidence.json', 'native-result.json', None),),
              'native-evidence.json'),
             ('build workflow missing native functional consumer host',
              (('  native-functional-consumers:', '  native-functional-consumers-disabled:', 1),),
              "missing mandatory job 'native-functional-consumers'"),
             ('build workflow native functional consumer missing a required suite',
              (('--test editor_bridge --test rebase_interactive', '--test rebase_interactive', 1),),
              'must include'),
             ('build workflow native functional consumer substituting a server build',
              (('nix develop --no-update-lock-file .#release-native -c "$make_bin" test-rust-artifact',
                'cargo build --release', 1),),
              'must not substitute a server build'),
             ('build workflow native functional consumer optional',
              (('  native-functional-consumers:\n',
                '  native-functional-consumers:\n    continue-on-error: true\n', 1),),
              'must be mandatory'),
             ('build workflow native functional consumer without post-run identity',
              (('test "$after" = "$expected"', 'true', 1),),
              'hash must remain unchanged'),
             ('build workflow native functional consumer without Neovim target',
               (('native-neovim-artifact-check', 'missing-neovim-target', 1),),
               'native-neovim-artifact-check'),
             ('build workflow missing native VSCode host',
              (('  native-vscode-consumers:', '  native-vscode-consumers-disabled:', 1),),
              "missing mandatory job 'native-vscode-consumers'"),
             ('build workflow native VSCode job optional',
              (('  native-vscode-consumers:\n',
                '  native-vscode-consumers:\n    continue-on-error: true\n', 1),),
              'native VSCode consumers must be mandatory'),
             ('build workflow native VSCode skips VSIX producer',
              (('needs: [native-candidates, vsix]', 'needs: native-candidates', 1),),
              'must need native and VSIX producers'),
             ('build workflow native VSCode misses immutable identity',
              (('vsix-candidate.json', 'missing-identity.json', 1),),
              "must include 'vsix-candidate.json'"),
             ('build workflow native VSCode imports the extractor from the downloader',
              (('from scripts.check_native_artifact import safe_extract',
                'from scripts.download_native_candidate import safe_extract', 1),),
              'must include'),
             ('build workflow native VSCode misses post-run hash',
              (('test "$vsix_before" = "$vsix_after"', 'true', 1),),
              "must include 'test \"$vsix_before\" = \"$vsix_after\"'"),
             ('build workflow with a lost readiness dependency',
              (('      - native-consumers\n', '', 1),),
              'must need exactly'),
             ('build workflow without the readiness success guard',
              (('          check native-consumers "$NATIVE_CONSUMERS_RESULT"',
                '          check native-consumers success', 1),),
              'must fail unless native-consumers succeeded'),
             ('build workflow masking a release-readiness failure',
              (('  release-readiness:\n',
                '  release-readiness:\n    continue-on-error: true\n', 1),),
              'must be mandatory'),
             ('build workflow without the VSIX gate always condition',
              (('    needs: [vsix, native-vscode-consumers]\n    if: always()',
                '    needs: [vsix, native-vscode-consumers]', 1),),
              'must run with `if: always()`'),
             ('build workflow VSIX gate losing its consumer dependency',
              (('    needs: [vsix, native-vscode-consumers]',
                '    needs: [vsix]', 1),),
              'must need exactly'),
             ('build workflow gating readiness on Windows',
              (('  release-readiness:\n    # Single fail-closed readiness aggregate',
                '  release-readiness:\n    runs-on: windows-2022\n'
                '    # Single fail-closed readiness aggregate', 1),),
              'Windows cannot gate'),
             ('build workflow with an aggregate that cannot fail',
              ((f'          check() {{ if [ "$2" != success ]; then echo '
                f'"required job $1 is \'$2\'" >&2; exit 1; fi; }}',
                '          check() { true; }', None),),
              'non-zero for any required result'),
           ):
            expect_check(label, check_build_workflow,
                         mutated_file(BUILD_WORKFLOW, directory, label, mutations),
                         expected=expected)

        # The same-job server graph: real command ordering, the exact packaged
        # release selector/hash, server-only duplication, coverage separation,
        # one-time consumer acceptance and mandatory GNU/macOS-only gating.
        debug_run = '        run: make test-rust FILTER="--locked --test server_async"\n'
        build_run = f'        run: {PIPELINE_BUILD}\n'
        release_make = ('          make test-rust-artifact '
                        'MAGIT_LSP_TEST_BINARY="$binary" FILTER="--test server_async"\n')
        package_run = f'        run: {PIPELINE_ARTIFACTS}\n'
        for label, mutations, expected in (
            ('build workflow after swapping the debug and build commands',
             ((debug_run, '        run: __TMP_DEBUG_RUN__\n', 1),
              (build_run, debug_run, 1),
              ('        run: __TMP_DEBUG_RUN__\n', build_run, 1)),
             'must run debug server tests before the release build'),
            ('build workflow testing the release inside the packaging step',
             ((release_make, '', 1),
              (package_run, package_run + release_make, 1)),
             'release server tests after packaging'),
            ('build workflow testing an unselected release target',
             (('target/x86_64-unknown-linux-gnu/release/magit-lsp',
               'target/aarch64-apple-darwin/release/magit-lsp', 1),),
             'exact packaged executable'),
            ('build workflow without the exact executable hash',
             (('["executable_sha256"])', '["archive_sha256"])', 1),),
             'executable_sha256'),
            ('build workflow repeating a nonserver suite in the debug step',
             (('FILTER="--locked --test server_async"',
               'FILTER="--locked --test server_async --test editor_bridge"', 1),),
             'must not repeat client/ABI/package acceptance'),
            ('build workflow mixing coverage into the server step',
             ((debug_run, debug_run.rstrip('\n') + ' LLVM_PROFILE_FILE=coverage.profraw\n', 1),),
             'mix coverage'),
            ('build workflow repeating ABI acceptance in the build job',
             (('      - id: upload\n',
               '      - run: python3 scripts/check_native_artifact.py --functional candidate.tar.gz\n'
               '      - id: upload\n', 1),),
             'must not repeat client/ABI/package acceptance'),
            ('build workflow gating on a Windows matrix row',
             (('          - os: macos-26\n'
               '            target: aarch64-apple-darwin\n'
               '            artifact: native-macos-arm64-experimental',
               '          - os: windows-latest\n'
               '            target: x86_64-pc-windows-msvc\n'
               '            artifact: native-windows-experimental\n'
               '          - os: macos-26\n'
               '            target: aarch64-apple-darwin\n'
               '            artifact: native-macos-arm64-experimental', 1),),
             'Windows cannot gate'),
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
    check_consumer_entrypoints()
    check_release_pipeline()
    check_local_pipeline()
    check_automation_unit()
    check_artifact_consistency()
    print('ok: release automation unit guards (no Cargo metadata or package)')


def build_parser():
    """Return the CLI parser without the retired readiness entrypoints."""
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
    parser.add_argument('--write-vsix-binding', action='store_true',
                        help='write exact VSIX hash and run provenance')
    parser.add_argument('--write-vsix-upload-identity', action='store_true',
                        help='write immutable VSIX artifact upload identity')
    parser.add_argument('--pipeline-self-tests', action='store_true',
                        help='run only the release pipeline adapter self-tests '
                             '(fake processes; no Cargo, build or metadata)')
    parser.add_argument('--automation-unit', action='store_true',
                        help='run the static/self-test guards without Cargo '
                             'metadata or package')
    parser.add_argument('--validate-publication-binding', metavar='BUNDLE',
                        help='validate an offline JSON run/tag/asset binding '
                             'bundle and print its source commit')
    parser.add_argument('--prepare-publication-runtime', metavar='EVENT',
                        help='read-only validate workflow_run event and upstream API')
    return parser


def main():
    args = build_parser().parse_args()
    try:
        if args.pipeline_self_tests:
            run_pipeline_self_tests()
            return 0
        if args.automation_unit:
            run_automation_unit()
            return 0
        if args.write_vsix_binding:
            write_vsix_binding()
            return 0
        if args.write_vsix_upload_identity:
            write_vsix_upload_identity()
            return 0
        if args.validate_publication_binding:
            validate_publication_binding_bundle(args.validate_publication_binding)
            return 0
        if args.prepare_publication_runtime:
            prepare_publication_runtime(args.prepare_publication_runtime)
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
