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
"""

import argparse
import hashlib
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
PIPELINE_ARTIFACTS = 'python3 scripts/check_release_pipeline.py artifacts'
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
SECRETS = ('VSCE_PAT', 'OVSX_PAT', 'CARGO_REGISTRY_TOKEN')
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
    for secret in SECRETS:

        if f'{secret}: ${{{{ secrets.{secret} }}}}' not in publish_text:
            fail(f'workflow: publish job must read secrets.{secret} into env')
        if f"env.{secret} != ''" not in publish_text:
            fail(f'workflow: {secret} lane must be gated on env.{secret} != empty')
        if f"env.{secret} == ''" not in publish_text:
            fail(f'workflow: {secret} lane must report a readable skip when unset')
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


def pipeline_violations(module):
    """Return the contract violations of a release pipeline module."""
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
    steps, environment = module.artifact_commands('1.2.3', 'binary', 'dist')
    if steps[:1] != [['sh', 'scripts/package-release.sh', '1.2.3', 'binary', 'dist']]:
        problems.append('artifact steps must package the explicit binary with '
                        'scripts/package-release.sh')
    if len(steps) < 3 or steps[1][-2:] != ['--validate-artifacts', 'dist']:
        problems.append('artifact steps must validate the archive set offline')
    if len(steps) < 3 or steps[-1][1:4] != ['-m', 'unittest', 'discover']:
        problems.append('artifact steps must run the release suite')
    if environment.get('MAGIT_LSP_TEST_BINARY') != 'binary':
        problems.append('artifact environment must hand the binary to '
                        'MAGIT_LSP_TEST_BINARY')
    return problems


def check_release_pipeline():
    """The local pipeline script must keep the workflow's pinned build/artifact contract."""
    problems = pipeline_violations(load_pipeline())
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
    """The Make targets must run the same pipeline script as the workflow."""
    lines = read_lines(path)
    for target, command in (
        ('release-locked', PIPELINE_BUILD),
        ('release-artifact-check', PIPELINE_ARTIFACTS),
    ):
        number = next((index for index, line in enumerate(lines, 1)
                       if line.startswith(target + ':')), None)
        if number is None:
            fail(f'Makefile: missing {target}')
        recipe = make_recipe(lines, number)
        if not any(command in line for line in recipe):
            fail(f'Makefile: {target} must run {command!r}')
    find_line(lines, 'release-artifact-check: release-locked', 'Makefile')


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
            ('publish workflow without frozen CLI dependencies',
             (('      - name: Install frozen VS Code CLI dependencies\n'
               '        run: make vscode-deps\n', '', None),),
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
        ):
            expect_check(label, check_build_workflow,
                         mutated_file(BUILD_WORKFLOW, directory, label, mutations),
                         expected=expected)

        # The local Make targets must run that same script, not a parallel path.
        for label, mutations in (
            ('Makefile without the pinned build target',
             ((PIPELINE_BUILD, 'cargo build --release', None),)),
            ('Makefile without the artifact target',
             ((PIPELINE_ARTIFACTS, 'true', None),)),
        ):
            expect_check(label, check_local_pipeline,
                         mutated_file(MAKEFILE, directory, label, mutations),
                         expected='Makefile:')

        # The shared script keeps the locked flags and the binary handoff.
        for label, mutations, expected in (
            ('pipeline without --locked',
             (("'--locked', ", '', None),), 'build command is'),
            ('pipeline without the binary handoff',
             (("'MAGIT_LSP_TEST_BINARY': str(binary),", '', None),),
             'MAGIT_LSP_TEST_BINARY'),
        ):
            problems = pipeline_violations(
                load_pipeline(mutated_file(PIPELINE, directory, label, mutations)))
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
    args = parser.parse_args()
    try:
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
