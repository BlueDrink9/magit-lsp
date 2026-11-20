#!/usr/bin/env python3
"""Check the release build/publication workflows and Cargo publish metadata.

The default mode is a static, line-oriented check (PyYAML is not available) of
``.github/workflows/release.yml`` and ``.github/workflows/publish.yml`` plus
`cargo package --list`, so an accidental auto-publish, a publication step in the
build workflow or an incomplete crate cannot land unnoticed. The helper modes
are invoked by the workflows themselves: ``--validate-release-ref`` validates
the version pair and tag ref for a build run, ``--validate-artifacts`` verifies
the downloaded release archive/sidecar and version triple, and
``--print-version`` emits the ``RELEASE_VERSION`` environment line for later
jobs.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

WORKFLOW = Path('.github/workflows/publish.yml')
BUILD_WORKFLOW = Path('.github/workflows/release.yml')
CARGO_MANIFEST = Path('Cargo.toml')
EXTENSION_MANIFEST = Path('clients/vscode/package.json')
ARTIFACT_NAME = 'linux-x86_64-gnu-release-candidate'
ASSET = 'x86_64-unknown-linux-gnu'
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


def check_workflow():
    lines = read_lines(WORKFLOW)
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
    if 'python3 scripts/check_release_automation.py --validate-artifacts' not in validate_text:
        fail('workflow: validate job must verify the archive and version triple')
    if 'make vscode-package' not in validate_text:
        fail('workflow: validate job must build the offline VSIX')
    if 'name: validated-release-assets' not in validate_text:
        fail('workflow: validate job must hand off validated assets to publish')

    # Publish lanes: mode gate plus credential presence, one readable skip per lane.
    publish_text = '\n'.join(publish)
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


def check_build_workflow():
    """Structure-check the tracked build workflow: dispatch/tag triggers, no publishing."""
    lines = read_lines(BUILD_WORKFLOW)
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

    # Build-only: no publication step and no publishing credential.
    for command in PUBLISH_COMMANDS + ('gh release create',):
        if command in text:
            fail(f'build workflow: publication step {command!r} is not allowed')
    if re.search(r'\bsecrets\.', text):
        fail('build workflow: must not read publishing secrets')
    for forbidden in RELEASE_FORBIDDEN_PATHS:
        if forbidden in text:
            fail(f'build workflow: references operator-owned path {forbidden!r}')


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


def validate_artifacts(directory):
    version = release_versions()
    root = Path(directory)
    stem = f'magit-lsp-v{version}-{ASSET}'
    archive = root / f'{stem}.tar.gz'
    sidecar = root / f'{archive.name}.sha256'
    if not archive.is_file():
        fail(f'artifacts: missing {archive}')
    if not sidecar.is_file():
        fail(f'artifacts: missing {sidecar}')
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--validate-artifacts', metavar='DIR',
                        help='verify the downloaded release archive and sidecar')
    parser.add_argument('--validate-release-ref', action='store_true',
                        help='validate RELEASE_REF and the version pair, writing $GITHUB_ENV')
    parser.add_argument('--print-version', action='store_true',
                        help='print RELEASE_VERSION=<X.Y.Z> for $GITHUB_ENV')
    args = parser.parse_args()
    try:
        if args.validate_artifacts:
            validate_artifacts(args.validate_artifacts)
            return 0
        if args.validate_release_ref:
            validate_release_ref()
            return 0
        if args.print_version:
            print(f'RELEASE_VERSION={release_versions()}')
            return 0
        check_workflow()
        check_build_workflow()
        check_artifact_consistency()
        check_metadata()
    except CheckError as error:
        print(f'FAILED: {error}', file=sys.stderr)
        return 1
    print('ok: release build/publish workflow guards and Cargo package metadata')
    return 0


if __name__ == '__main__':
    sys.exit(main())
