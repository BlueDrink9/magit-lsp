#!/usr/bin/env python3
"""Run the release workflow's locked build and actual-artifact checks locally.

``build`` refuses any compiler that is not exactly the pinned Rust toolchain and
then runs the workflow's ``--release --locked --target`` build; ``build
--local`` instead uses the installed compiler without downloading one and
records its real versions, while keeping the same locked command and artifact
path. ``build-native`` is the experimental hosted native-linker lane: it accepts
only the Linux x86_64 host and target, points Cargo's target linker explicitly
at ``/usr/bin/cc`` and records the exact locked argv plus compiler, linker, libc
and source/lock provenance under ``build/release/``. It never changes or
replaces the strict pinned/``--local`` commands. ``artifacts``
packages that exact binary with ``scripts/package-release.sh``, validates the
archive/sidecar set offline with the release checker and runs the release suite
with ``MAGIT_LSP_TEST_BINARY`` set, so the real-binary case runs instead of
skipping. ``.github/workflows/release.yml`` invokes this same script, so the
local targets are the workflow's steps rather than a parallel implementation.

A missing pinned compiler fails with a readable message; this script never
downloads or installs a toolchain and never publishes anything.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
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
# The experimental native lane supports only the hosted Linux GNU target; Darwin
# selection is a separate task. The host compiler/linker is always the explicit
# absolute host path, never a Nix wrapper discovered on PATH.
NATIVE_HOST = ('linux', 'x86_64')
NATIVE_CC = '/usr/bin/cc'
NATIVE_LINKER_VARIABLE = 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'
NATIVE_EVIDENCE_DIR = Path('build/release')
NATIVE_EVIDENCE_FILE = 'native-build.json'
# Inherited settings that would inject a C compiler, linker or sysroot from a
# Nix dev shell or a caller wrapper. The native lane refuses them instead of
# trusting flags the release shell happened to export.
NATIVE_INJECTION_VARIABLES = (
    'CC', 'CXX', 'AR', 'CFLAGS', 'CPPFLAGS', 'LDFLAGS',
    'NIX_CC', 'NIX_CFLAGS_COMPILE', 'NIX_LDFLAGS', 'SDKROOT',
)


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


def host_platform():
    """Return the running host as a lower-cased ``(system, machine)`` pair."""
    return (platform.system().lower(), platform.machine().lower())


def run_probe(argv, environment=None):
    """Run one host probe from the repository root and return (code, stdout, stderr)."""
    result = subprocess.run(
        argv, cwd=ROOT, env=process_environment(environment),
        capture_output=True, text=True)
    return result.returncode, result.stdout, result.stderr


def inherited_injection(environment):
    """Return inherited compiler/linker/sysroot settings the native lane refuses.

    A Nix dev shell or caller wrapper can export ``CC``, target linker or
    sysroot flags; the native lane sets its own explicit linker and refuses the
    rest rather than trusting them. Only the exact host linker value is allowed
    through the target linker variable.
    """
    names = [name for name in NATIVE_INJECTION_VARIABLES if environment.get(name)]
    for name, value in environment.items():
        if not value:
            continue
        if name.startswith('CARGO_TARGET_') and name.endswith(('_LINKER', '_RUSTFLAGS')):
            if name == NATIVE_LINKER_VARIABLE and value == NATIVE_CC:
                continue
            names.append(name)
        elif name in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS') and \
                re.search(r'sysroot|linker|link-arg', value):
            names.append(name)
    return sorted(set(names))


def native_cargo(environment, probe):
    """Return the installed ``cargo`` prefix and versions, or fail readably."""
    versions = {}
    for tool in ('rustc', 'cargo'):
        code, stdout, stderr = probe([tool, '--version'], environment)
        line = stdout.strip() or stderr.strip()
        if code != 0 or not line:
            raise PipelineError(
                f'{tool} --version failed; run the native lane inside '
                '`nix develop --no-update-lock-file .#release-native -c`')
        versions[tool] = line
    return ['cargo'], versions


def native_plan(target, environment, host=None, compiler_path=NATIVE_CC,
                probe=run_probe):
    """Return the validated native build plan and its provenance evidence.

    The plan carries the strict ``--release --locked --target`` argv, the
    environment with Cargo's target linker set explicitly to the host compiler,
    and the compiler/linker/libc/argv/source-SHA/lock-hash record written under
    ``build/release/``. ``host`` and ``probe`` are injectable so the self-tests
    exercise the refusals and recorded values with fake processes, never a real
    compiler, Cargo or build.
    """
    host = host or host_platform()
    if host != NATIVE_HOST:
        raise PipelineError(
            f'native link mode needs a Linux x86_64 host; found {host[0]} {host[1]}')
    if not target:
        raise PipelineError(
            'build-native needs RELEASE_TARGET (for example '
            f'RELEASE_TARGET={TARGET})')
    if target != TARGET:
        raise PipelineError(
            f'native link mode only supports {TARGET}; got {target!r}')
    injected = inherited_injection(environment)
    if injected:
        raise PipelineError(
            'refusing inherited compiler/linker/sysroot settings: ' + ', '.join(injected))
    compiler = Path(compiler_path)
    if not compiler.is_file() or not os.access(compiler, os.X_OK):
        raise PipelineError(
            f'{compiler_path} is missing or not executable; the native lane needs '
            'the host C compiler and linker')
    resolved = compiler.resolve()
    if str(resolved).startswith('/nix/store/'):
        raise PipelineError(
            f'{compiler_path} resolves into the Nix store ({resolved}); point it at '
            'the host C compiler')
    code, stdout, stderr = probe([compiler_path, '--version'], environment)
    compiler_version = stdout.strip() or stderr.strip()
    if code != 0 or not compiler_version:
        raise PipelineError(
            f'{compiler_path} --version failed; the native lane needs a working '
            'host C compiler')
    code, stdout, stderr = probe(['ldd', '--version'], environment)
    libc = stdout.strip().splitlines()[0] if stdout.strip() else ''
    if code != 0 or not libc:
        raise PipelineError('ldd --version failed; the native lane needs the host libc')
    code, stdout, stderr = probe(['git', 'rev-parse', 'HEAD'], environment)
    source_sha = stdout.strip()
    if code != 0 or not re.fullmatch(r'[0-9a-f]{40}', source_sha):
        raise PipelineError(
            'git rev-parse HEAD failed; the native evidence needs the source SHA')
    lock_hashes = {}
    for lock in ('Cargo.lock', 'flake.lock'):
        path = ROOT / lock
        if not path.is_file():
            raise PipelineError(f'{lock} is missing; the native evidence needs its hash')
        lock_hashes[lock] = hashlib.sha256(path.read_bytes()).hexdigest()
    cargo_prefix, versions = native_cargo(environment, probe)
    merged = dict(environment)
    merged[NATIVE_LINKER_VARIABLE] = NATIVE_CC
    argv = build_command(cargo_prefix)
    evidence = {
        'target': target,
        'host': {'system': host[0], 'machine': host[1]},
        'compiler': compiler_version,
        'linker': NATIVE_CC,
        'libc': libc,
        'argv': argv,
        'source_sha': source_sha,
        'lock_hashes': lock_hashes,
        'rustc': versions['rustc'],
        'cargo': versions['cargo'],
        'linker_environment': {NATIVE_LINKER_VARIABLE: NATIVE_CC},
    }
    return {'argv': argv, 'environment': merged, 'evidence': evidence}


def write_evidence(evidence, directory=None):
    """Write the native build provenance JSON under ``build/release/``."""
    root = Path(directory) if directory is not None else ROOT / NATIVE_EVIDENCE_DIR
    root.mkdir(parents=True, exist_ok=True)
    path = root / NATIVE_EVIDENCE_FILE
    path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + '\n',
                    encoding='utf-8')
    return path


def native_build(target, environment):
    """Run the experimental native-linker release build and record its provenance."""
    plan = native_plan(target, environment)
    path = write_evidence(plan['evidence'])
    print(f'native release evidence: {path}', file=sys.stderr)
    run(plan['argv'], plan['environment'])


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
    parser.add_argument('mode', choices=('build', 'artifacts', 'build-native'),
                        help='build the pinned binary, package and smoke-test it, '
                             'or run the experimental native-linker build')
    parser.add_argument('--local', action='store_true',
                        help='build with the installed compiler instead of the '
                             'pinned toolchain (never downloads one)')
    parser.add_argument('--target', default='',
                        help='RELEASE_TARGET for build-native; must match the host')
    args = parser.parse_args()
    try:
        if args.mode == 'build':
            build(args.local, os.environ)
        elif args.mode == 'build-native':
            if args.local:
                raise PipelineError(
                    'build-native is a separate experimental lane; it never combines '
                    'with the pinned/--local modes')
            native_build(args.target, os.environ)
        else:
            artifacts(os.environ)
    except PipelineError as error:
        print(f'release-pipeline: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
