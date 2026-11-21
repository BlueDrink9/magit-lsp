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
shell.
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
    """Every `uses:` needs a full action sha plus a `# vX.Y.Z` comment."""
    for number, line in enumerate(lines, 1):
        match = re.match(r'^\s*(?:-\s*)?uses:\s*(\S+)(.*)$', line)
        if not match:
            continue
        reference, rest = match.groups()
        if not re.fullmatch(r'[\w.\-]+/[\w.\-]+@[0-9a-f]{40}', reference):
            fail(f'{context}:{number}: action {reference!r} is not pinned to a full sha')
        if not re.search(r'#\s*v\d+\.\d+\.\d+\s*$', rest):
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


def check_python_interpreter(lines):
    """Require a pinned setup-python >= 3.11 step before the first python3 use."""
    consumers = [index for index, line in enumerate(lines)
                 if PYTHON_CONSUMER.search(line) and not line.lstrip().startswith('#')]
    if not consumers:
        fail('build workflow: no python3 consumer found to order against')
    setup_at = next((index for index, line in enumerate(lines)
                     if f'uses: {SETUP_PYTHON}' in line), None)
    if setup_at is None:
        fail('build workflow: missing pinned actions/setup-python step')
    if setup_at > consumers[0]:
        fail('build workflow: actions/setup-python must precede the first python3 '
             f'step (line {consumers[0] + 1})')
    version = None
    for entry in block_after(lines, setup_at, indent_of(lines[setup_at])):
        match = re.match(r"^\s*python-version:\s*['\"]?(\d+(?:\.\d+)*)", entry)
        if match:
            version = tuple(int(part) for part in match.group(1).split('.'))
            break
    if not version:
        fail('build workflow: actions/setup-python must select an explicit '
             'python-version')
    if version < MIN_PYTHON:
        selected = '.'.join(str(part) for part in version)
        required = '.'.join(str(part) for part in MIN_PYTHON)
        fail(f'build workflow: selected Python {selected} is below the required '
             f'{required}')


def step_artifact_name(lines, action, context):
    """Return the `name:` of the first step that uses ``action``."""
    for index, line in enumerate(lines):
        if f'uses: {action}' not in line:
            continue
        for candidate in lines[index + 1:]:
            if candidate.strip() and indent_of(candidate) <= indent_of(line):
                break
            match = re.match(r'^\s*name:\s*(\S+)\s*$', candidate)
            if match:
                return match.group(1)
        fail(f'{context}: {action} step has no artifact `name:`')
    fail(f'{context}: missing {action}')


def check_build_workflow(path=BUILD_WORKFLOW):
    """Structure-check the build workflow: dispatch/tag triggers, no publishing."""
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
    if 'contents: write' in text:
        fail('build workflow: must stay read-only (`contents: write` found)')

    check_pinned_actions(lines, 'build workflow')
    check_inline_scripts(lines, 'build workflow')
    check_python_interpreter(lines)

    # Release handoff: the fixed toolchain, the tag/version validator that
    # produces RELEASE_VERSION in $GITHUB_ENV, and the shared pipeline script
    # the local Make targets also run. An inline cargo/package command here
    # would be a second correctness path that the local target could not share.
    code = code_lines(lines)
    pipeline_text = '\n'.join(code)
    if f'rustup toolchain install {RELEASE_TOOLCHAIN} --profile minimal' not in pipeline_text:
        fail(f'build workflow: must install the pinned Rust {RELEASE_TOOLCHAIN} toolchain')
    if '--validate-release-ref' not in pipeline_text:
        fail('build workflow: must validate the version/tag ref before building '
             '(it produces RELEASE_VERSION for $GITHUB_ENV)')
    if PIPELINE_BUILD not in pipeline_text:
        fail(f'build workflow: must run the shared pinned build {PIPELINE_BUILD!r}')
    if PIPELINE_ARTIFACTS not in pipeline_text:
        fail(f'build workflow: must run the shared artifact check {PIPELINE_ARTIFACTS!r}')
    if '--local' in pipeline_text:
        fail('build workflow: must use the pinned toolchain, never the local '
             'installed-compiler mode')
    for alternate in ('cargo build', 'package-release.sh', 'MAGIT_LSP_TEST_BINARY'):
        if alternate in pipeline_text:
            fail(f'build workflow: {alternate!r} belongs to the shared pipeline script, '
                 'not a separate workflow path')
    ref_at = next(i for i, line in enumerate(code) if '--validate-release-ref' in line)
    build_at = next(i for i, line in enumerate(code) if PIPELINE_BUILD in line)
    artifacts_at = next(i for i, line in enumerate(code) if PIPELINE_ARTIFACTS in line)
    if not ref_at < build_at < artifacts_at:
        fail('build workflow: version validation must precede the pinned build, '
             'and the build must precede artifact packaging')

    # Build-only: no publication step and no publishing credential.
    for command in PUBLISH_COMMANDS + ('gh release create',):
        if command in text:
            fail(f'build workflow: publication step {command!r} is not allowed')
    if re.search(r'\bsecrets\.', text):
        fail('build workflow: must not read publishing secrets')
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
                 'package_plan', 'release_manifest', 'manifest_digest',
                 'write_manifest'):
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
    args = parser.parse_args()
    try:
        if args.pipeline_self_tests:
            run_pipeline_self_tests()
            return 0
        if args.automation_unit:
            run_automation_unit()
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
