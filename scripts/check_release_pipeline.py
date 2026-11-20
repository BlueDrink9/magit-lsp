#!/usr/bin/env python3
"""Run the release workflow's locked build and actual-artifact checks locally.

``build`` refuses any compiler that is not exactly the pinned Rust toolchain and
then runs the workflow's ``--release --locked --target`` build; ``build
--local`` instead uses the installed compiler without downloading one and
records its real versions, while keeping the same locked command and artifact
path. ``artifacts``
packages that exact binary with ``scripts/package-release.sh``, validates the
archive/sidecar set offline with the release checker and runs the release suite
with ``MAGIT_LSP_TEST_BINARY`` set, so the real-binary case runs instead of
skipping. ``.github/workflows/release.yml`` invokes this same script, so the
local targets are the workflow's steps rather than a parallel implementation.

A missing pinned compiler fails with a readable message; this script never
downloads or installs a toolchain and never publishes anything.
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
TOOLCHAIN = '1.94.1'
TARGET = 'x86_64-unknown-linux-gnu'
BINARY = Path('target') / TARGET / 'release' / 'magit-lsp'
DIST = Path('dist')
VALIDATOR = Path('scripts/check_release_automation.py')
VERSION_PATTERN = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)')


class PipelineError(Exception):
    """One failed release pipeline step, reported without a traceback."""


def build_command(cargo):
    """Return the lockstep build argv for a ``cargo`` prefix (for example ``['cargo', '+1.94.1']``)."""
    return list(cargo) + ['build', '--release', '--locked', '--target', TARGET]


def artifact_commands(version, binary, dist):
    """Return the ordered package/validate/test step argv list and their environment.

    ``binary`` is the actual release binary and ``dist`` the output directory.
    The environment carries the ``MAGIT_LSP_TEST_BINARY`` handoff the release
    suite needs to exercise packaging, installation and launch instead of
    skipping, plus bytecode suppression.
    """
    steps = [
        ['sh', 'scripts/package-release.sh', version, str(binary), str(dist)],
        [sys.executable, str(VALIDATOR), '--validate-artifacts', str(dist)],
        [sys.executable, '-m', 'unittest', 'discover', '-s', 'tests/release',
         '-p', 'test_*.py', '-v'],
    ]
    environment = {
        'MAGIT_LSP_TEST_BINARY': str(binary),
        'PYTHONDONTWRITEBYTECODE': '1',
    }
    return steps, environment


def manifest_version():
    """Return the shared Cargo/extension version, refusing a malformed or split pair."""
    package = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']
    version = package.get('version', '')
    if not VERSION_PATTERN.fullmatch(version):
        raise PipelineError(f'Cargo.toml version {version!r} is not a stable X.Y.Z version')
    extension = json.loads(
        (ROOT / 'clients/vscode/package.json').read_text(encoding='utf-8'))['version']
    if extension != version:
        raise PipelineError(
            f'Cargo.toml {version} and extension package.json {extension} versions differ')
    return version


def resolve_version(environment):
    """Return the packaging version, refusing a broken CI handoff.

    The build workflow's earlier ``--validate-release-ref`` step writes
    ``RELEASE_VERSION`` to ``$GITHUB_ENV``, so in CI the value must be present
    and match the manifests; locally it is absent and the manifests decide.
    """
    version = manifest_version()
    supplied = environment.get('RELEASE_VERSION')
    if environment.get('GITHUB_ACTIONS') and not supplied:
        raise PipelineError(
            'RELEASE_VERSION is missing; the --validate-release-ref step must publish it')
    if supplied and supplied.removeprefix('v') != version:
        raise PipelineError(
            f'RELEASE_VERSION {supplied} does not match the manifest version {version}')
    return version


def process_environment(environment):
    """Return ``environment`` layered over this process's environment."""
    return dict(os.environ, **(environment or {}))


def compiler(environment):
    """Return the cargo prefix for exactly the pinned toolchain, or fail readably."""
    found = 'no cargo/rustc on PATH'
    merged = process_environment(environment)
    if shutil.which('cargo', path=merged.get('PATH')) and \
            shutil.which('rustc', path=merged.get('PATH')):
        probe = subprocess.run(
            ['cargo', f'+{TOOLCHAIN}', '--version'], cwd=ROOT, env=merged,
            capture_output=True, text=True)
        if probe.returncode == 0:
            return ['cargo', f'+{TOOLCHAIN}']
        version = subprocess.run(
            ['rustc', '--version'], cwd=ROOT, env=merged,
            capture_output=True, text=True)
        found = version.stdout.strip() or version.stderr.strip() or found
        if version.returncode == 0 and version.stdout.split()[1:2] == [TOOLCHAIN]:
            return ['cargo']
    raise PipelineError(
        f'the pinned release build needs Rust {TOOLCHAIN}; found {found}. '
        f'Provide that exact toolchain and target {TARGET} (for example with '
        f'`rustup toolchain install {TOOLCHAIN}`); this target never downloads '
        'or installs a toolchain itself.')


def installed_prefix(environment):
    """Return the installed ``cargo`` prefix without downloading or pinning one."""
    merged = process_environment(environment)
    missing = [tool for tool in ('cargo', 'rustc')
               if not shutil.which(tool, path=merged['PATH'])]
    if missing:
        raise PipelineError(
            f'the local release build needs installed {" and ".join(missing)} on PATH; '
            'this target never downloads or installs a toolchain')
    return ['cargo']


def installed_compiler(environment):
    """Return the installed ``cargo`` prefix after recording the real toolchain."""
    prefix = installed_prefix(environment)
    versions = []
    for tool in ('rustc', 'cargo'):
        result = subprocess.run(
            [tool, '--version'], cwd=ROOT, env=process_environment(environment),
            capture_output=True, text=True)
        line = result.stdout.strip() or result.stderr.strip()
        if result.returncode != 0 or not line:
            raise PipelineError(f'{tool} --version failed; the local release build '
                                'needs a working installed toolchain')
        versions.append(line)
    print(f'local release toolchain: {versions[0]}; {versions[1]}', file=sys.stderr)
    print('note: local mode proves the installed compiler; the hosted workflow '
          f'keeps Rust {TOOLCHAIN} on Ubuntu 22.04 (glibc 2.35) for the exact '
          'binary/glibc baseline', file=sys.stderr)
    return prefix


def compiler_for(local, environment):
    """Return the cargo prefix: the installed compiler for ``local``, else the pin."""
    return installed_compiler(environment) if local else compiler(environment)


def run(argv, environment=None):
    """Run one step from the repository root, failing readably on a nonzero exit."""
    merged = dict(os.environ, **(environment or {}))
    print(f'+ {" ".join(argv)}', file=sys.stderr)
    result = subprocess.run(argv, cwd=ROOT, env=merged)
    if result.returncode != 0:
        raise PipelineError(f'{" ".join(argv)} exited {result.returncode}')


def build(local, environment):
    """Run the workflow's locked, target-specific release build.

    ``local`` selects the installed compiler (never downloading one) and records
    its actual versions; both modes issue the same ``--release --locked
    --target`` command and produce the binary ``artifacts`` later packages.
    """
    run(build_command(compiler_for(local, environment)), environment)


def artifacts(environment):
    """Package, validate and smoke-test the actual release binary the build produced."""
    version = resolve_version(environment)
    if not (ROOT / BINARY).is_file():
        raise PipelineError(f'{BINARY} is missing; run `make release-locked` first')
    steps, step_environment = artifact_commands(version, BINARY, DIST)
    for argv in steps:
        run(argv, step_environment)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('build', 'artifacts'),
                        help='build the pinned binary, or package and smoke-test it')
    parser.add_argument('--local', action='store_true',
                        help='build with the installed compiler instead of the '
                             'pinned toolchain (never downloads one)')
    args = parser.parse_args()
    try:
        if args.mode == 'build':
            build(args.local, os.environ)
        else:
            artifacts(os.environ)
    except PipelineError as error:
        print(f'release-pipeline: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
